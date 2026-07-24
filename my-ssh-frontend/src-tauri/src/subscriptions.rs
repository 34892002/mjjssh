use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{header, redirect, Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use tokio::net::lookup_host;

const CONFIG_FILE: &str = "script-subscriptions.json";
const CACHE_DIR: &str = "script-subscription-cache";
const FORMAT_VERSION: u32 = 1;
const OFFICIAL_ID: &str = "official";
const OFFICIAL_NAME: &str = "MJJ Scripts";
const OFFICIAL_URL: &str =
    "https://raw.githubusercontent.com/34892002/mjjssh/refs/heads/main/subs/sub_main.jsonl";
const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_LINE_BYTES: usize = 64 * 1024;
const MAX_RECORDS: usize = 1000;
const MAX_REDIRECTS: usize = 3;

#[derive(Debug, thiserror::Error)]
pub enum SubscriptionError {
    #[error("subscription storage error: {0}")]
    Storage(String),
    #[error("invalid subscription: {0}")]
    Invalid(String),
    #[error("subscription not found: {0}")]
    NotFound(String),
    #[error("official subscription cannot be removed")]
    OfficialCannotBeRemoved,
    #[error("subscription request failed: {0}")]
    Network(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub official: bool,
    pub etag: Option<String>,
    pub last_fetched_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddSubscriptionRequest {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSubscriptionRequest {
    pub name: Option<String>,
    pub url: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionScript {
    pub subscription_id: String,
    pub subscription_name: String,
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub command: String,
    pub risk_level: String,
    pub platforms: Vec<String>,
    pub version: String,
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseWarning {
    pub line: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshSubscriptionResult {
    pub subscription: Subscription,
    pub status: u16,
    pub not_modified: bool,
    pub scripts: Vec<SubscriptionScript>,
    pub warnings: Vec<ParseWarning>,
    pub used_cached_content: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionConfig {
    format_version: u32,
    subscriptions: Vec<Subscription>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonlScript {
    schema_version: u32,
    id: String,
    name: String,
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    command: String,
    risk_level: String,
    #[serde(default)]
    platforms: Vec<String>,
    version: String,
    homepage: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonlMetadata {
    schema_version: u32,
    #[serde(rename = "type")]
    record_type: String,
    name: String,
}

pub struct SubscriptionStore {
    app_dir: PathBuf,
}

impl SubscriptionStore {
    pub fn new(app_dir: impl Into<PathBuf>) -> Self {
        Self {
            app_dir: app_dir.into(),
        }
    }

    pub fn list(&self) -> Result<Vec<Subscription>, SubscriptionError> {
        Ok(self.load_config()?.subscriptions)
    }

    pub fn add(&self, request: AddSubscriptionRequest) -> Result<Subscription, SubscriptionError> {
        validate_subscription(&request.name, &request.url)?;
        let mut config = self.load_config()?;
        let subscription = Subscription {
            id: uuid::Uuid::new_v4().to_string(),
            name: request.name.trim().into(),
            url: request.url.trim().into(),
            enabled: true,
            official: false,
            etag: None,
            last_fetched_at: None,
            last_success_at: None,
            last_error: None,
        };
        config.subscriptions.push(subscription.clone());
        self.save_config(&config)?;
        Ok(subscription)
    }

    pub fn update(
        &self,
        id: &str,
        request: UpdateSubscriptionRequest,
    ) -> Result<Subscription, SubscriptionError> {
        let mut config = self.load_config()?;
        let subscription = config
            .subscriptions
            .iter_mut()
            .find(|subscription| subscription.id == id)
            .ok_or_else(|| SubscriptionError::NotFound(id.into()))?;
        let name = request.name.as_deref().unwrap_or(&subscription.name);
        let url = request.url.as_deref().unwrap_or(&subscription.url);
        validate_subscription(name, url)?;
        subscription.name = name.trim().into();
        subscription.url = url.trim().into();
        if let Some(enabled) = request.enabled {
            subscription.enabled = enabled;
        }
        if request.url.is_some() {
            subscription.etag = None;
        }
        let updated = subscription.clone();
        self.save_config(&config)?;
        Ok(updated)
    }

    pub fn remove(&self, id: &str) -> Result<(), SubscriptionError> {
        let mut config = self.load_config()?;
        let subscription = config
            .subscriptions
            .iter()
            .find(|subscription| subscription.id == id)
            .ok_or_else(|| SubscriptionError::NotFound(id.into()))?;
        if subscription.official {
            return Err(SubscriptionError::OfficialCannotBeRemoved);
        }
        config
            .subscriptions
            .retain(|subscription| subscription.id != id);
        self.save_config(&config)?;
        let cache = self.cache_path(id);
        if cache.exists() {
            fs::remove_file(cache).map_err(io_error)?;
        }
        Ok(())
    }

    pub fn cached_scripts(
        &self,
        id: &str,
    ) -> Result<(Vec<SubscriptionScript>, Vec<ParseWarning>), SubscriptionError> {
        let config = self.load_config()?;
        let subscription = config
            .subscriptions
            .iter()
            .find(|subscription| subscription.id == id)
            .ok_or_else(|| SubscriptionError::NotFound(id.into()))?;
        let raw = fs::read_to_string(self.cache_path(id)).map_err(io_error)?;
        Ok(parse_jsonl(subscription, &raw))
    }

    pub async fn refresh(&self, id: &str) -> Result<RefreshSubscriptionResult, SubscriptionError> {
        let mut config = self.load_config()?;
        let index = config
            .subscriptions
            .iter()
            .position(|subscription| subscription.id == id)
            .ok_or_else(|| SubscriptionError::NotFound(id.into()))?;
        let subscription = config.subscriptions[index].clone();
        if !subscription.enabled {
            return Err(SubscriptionError::Invalid(
                "subscription is disabled".into(),
            ));
        }

        let outcome = fetch_jsonl(&subscription).await;
        let result = match outcome {
            Ok(FetchOutcome::NotModified) => {
                let (scripts, warnings) = self.cached_scripts(id)?;
                config.subscriptions[index].last_fetched_at = Some(now());
                config.subscriptions[index].last_error = None;
                RefreshSubscriptionResult {
                    subscription: config.subscriptions[index].clone(),
                    status: 304,
                    not_modified: true,
                    scripts,
                    warnings,
                    used_cached_content: true,
                }
            }
            Ok(FetchOutcome::Downloaded { body, etag, status }) => {
                let metadata = parse_metadata(&body);
                let (scripts, warnings) = parse_jsonl(&subscription, &body);
                if scripts.is_empty()
                    || warnings
                        .iter()
                        .any(|warning| warning.reason == "subscription exceeds 1,000 scripts")
                {
                    let reason = if scripts.is_empty() {
                        "no valid scripts in subscription"
                    } else {
                        "subscription exceeds 1,000 scripts"
                    };
                    return self.refresh_failure(config, index, reason.into());
                }
                self.write_cache(id, body.as_bytes())?;
                let subscription = &mut config.subscriptions[index];
                if subscription.official {
                    if let Some(metadata) = metadata {
                        subscription.name = metadata.name;
                    }
                }
                subscription.etag = etag;
                subscription.last_fetched_at = Some(now());
                subscription.last_success_at = subscription.last_fetched_at.clone();
                subscription.last_error = None;
                RefreshSubscriptionResult {
                    subscription: subscription.clone(),
                    status,
                    not_modified: false,
                    scripts,
                    warnings,
                    used_cached_content: false,
                }
            }
            Err(error) => return self.refresh_failure(config, index, error.to_string()),
        };
        self.save_config(&config)?;
        Ok(result)
    }

    fn refresh_failure(
        &self,
        mut config: SubscriptionConfig,
        index: usize,
        error: String,
    ) -> Result<RefreshSubscriptionResult, SubscriptionError> {
        config.subscriptions[index].last_fetched_at = Some(now());
        config.subscriptions[index].last_error = Some(error);
        let subscription = config.subscriptions[index].clone();
        self.save_config(&config)?;
        match self.cached_scripts(&subscription.id) {
            Ok((scripts, warnings)) => Ok(RefreshSubscriptionResult {
                subscription,
                status: 0,
                not_modified: false,
                scripts,
                warnings,
                used_cached_content: true,
            }),
            Err(_) => Err(SubscriptionError::Network(
                subscription
                    .last_error
                    .unwrap_or_else(|| "subscription refresh failed".into()),
            )),
        }
    }

    fn load_config(&self) -> Result<SubscriptionConfig, SubscriptionError> {
        fs::create_dir_all(&self.app_dir).map_err(io_error)?;
        let path = self.app_dir.join(CONFIG_FILE);
        let config = if path.exists() {
            let config: SubscriptionConfig =
                serde_json::from_slice(&fs::read(&path).map_err(io_error)?).map_err(|error| {
                    SubscriptionError::Invalid(format!("configuration JSON: {error}"))
                })?;
            if config.format_version != FORMAT_VERSION {
                return Err(SubscriptionError::Invalid(
                    "unsupported configuration formatVersion".into(),
                ));
            }
            config
        } else {
            SubscriptionConfig {
                format_version: FORMAT_VERSION,
                subscriptions: vec![official_subscription()],
            }
        };
        if !path.exists() {
            self.save_config(&config)?;
        }
        Ok(config)
    }

    fn save_config(&self, config: &SubscriptionConfig) -> Result<(), SubscriptionError> {
        write_atomic(
            &self.app_dir.join(CONFIG_FILE),
            &serde_json::to_vec_pretty(config)
                .map_err(|error| SubscriptionError::Storage(error.to_string()))?,
        )
    }

    fn write_cache(&self, id: &str, content: &[u8]) -> Result<(), SubscriptionError> {
        let path = self.cache_path(id);
        let parent = path.parent().expect("cache path has a parent");
        fs::create_dir_all(parent).map_err(io_error)?;
        write_atomic(&path, content)
    }

    fn cache_path(&self, id: &str) -> PathBuf {
        self.app_dir.join(CACHE_DIR).join(format!("{id}.jsonl"))
    }
}

enum FetchOutcome {
    NotModified,
    Downloaded {
        body: String,
        etag: Option<String>,
        status: u16,
    },
}

async fn fetch_jsonl(subscription: &Subscription) -> Result<FetchOutcome, SubscriptionError> {
    let mut target = parse_safe_url(&subscription.url).await?;
    for redirects in 0..=MAX_REDIRECTS {
        let client = safe_client(&target)?;
        let mut request = client.get(target.url.clone()).header(
            header::ACCEPT,
            "application/x-ndjson, application/jsonl, text/plain",
        );
        if let Some(etag) = &subscription.etag {
            request = request.header(header::IF_NONE_MATCH, etag);
        }
        let response = request
            .send()
            .await
            .map_err(|error| SubscriptionError::Network(error.to_string()))?;
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(FetchOutcome::NotModified);
        }
        if response.status().is_redirection() {
            if redirects == MAX_REDIRECTS {
                return Err(SubscriptionError::Network("too many redirects".into()));
            }
            let location = response
                .headers()
                .get(header::LOCATION)
                .ok_or_else(|| SubscriptionError::Network("redirect without Location".into()))?
                .to_str()
                .map_err(|_| SubscriptionError::Network("invalid redirect Location".into()))?;
            target = parse_safe_url(
                target
                    .url
                    .join(location)
                    .map_err(|_| SubscriptionError::Network("invalid redirect URL".into()))?
                    .as_str(),
            )
            .await?;
            continue;
        }
        if !response.status().is_success() {
            return Err(SubscriptionError::Network(format!(
                "HTTP {}",
                response.status()
            )));
        }
        let status = response.status().as_u16();
        let etag = response
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| SubscriptionError::Network(error.to_string()))?;
            if body.len().saturating_add(chunk.len()) > MAX_FILE_BYTES {
                return Err(SubscriptionError::Network("response exceeds 1 MiB".into()));
            }
            body.extend_from_slice(&chunk);
        }
        let body = String::from_utf8(body)
            .map_err(|_| SubscriptionError::Network("response is not UTF-8".into()))?;
        return Ok(FetchOutcome::Downloaded { body, etag, status });
    }
    unreachable!()
}

struct SafeUrl {
    url: Url,
    address: SocketAddr,
}

fn safe_client(target: &SafeUrl) -> Result<Client, SubscriptionError> {
    let host = target.url.host_str().expect("safe URL has a host");
    Client::builder()
        .redirect(redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(45))
        .resolve(host, target.address)
        .build()
        .map_err(|error| SubscriptionError::Network(error.to_string()))
}

async fn parse_safe_url(value: &str) -> Result<SafeUrl, SubscriptionError> {
    let url = Url::parse(value.trim())
        .map_err(|_| SubscriptionError::Invalid("URL is invalid".into()))?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err(SubscriptionError::Invalid(
            "URL must be a credential-free HTTPS URL".into(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| SubscriptionError::Invalid("URL must have a host".into()))?;
    let addresses: Vec<SocketAddr> =
        lookup_host((host, url.port_or_known_default().unwrap_or(443)))
            .await
            .map_err(|error| SubscriptionError::Network(format!("DNS lookup failed: {error}")))?
            .collect();
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| !is_public_address(address.ip()))
    {
        return Err(SubscriptionError::Invalid(
            "URL resolves to a non-public address".into(),
        ));
    }
    Ok(SafeUrl {
        url,
        address: addresses[0],
    })
}

fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            !(address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_unspecified()
                || address.is_broadcast()
                || address.is_multicast()
                || address.octets()[0] == 0
                || (address.octets()[0] == 100 && (64..=127).contains(&address.octets()[1]))
                || (address.octets()[0] == 198 && matches!(address.octets()[1], 18 | 19)))
        }
        IpAddr::V6(address) => {
            !(address.is_loopback()
                || address.is_unspecified()
                || address.is_multicast()
                || address.is_unicast_link_local()
                || (address.segments()[0] & 0xfe00) == 0xfc00)
        }
    }
}

fn parse_jsonl(
    subscription: &Subscription,
    raw: &str,
) -> (Vec<SubscriptionScript>, Vec<ParseWarning>) {
    let mut scripts = Vec::new();
    let mut warnings = Vec::new();
    let mut ids = HashSet::new();
    for (line_index, line) in raw.trim_start_matches('\u{feff}').lines().enumerate() {
        let line_number = line_index + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.len() > MAX_LINE_BYTES {
            warnings.push(ParseWarning {
                line: line_number,
                reason: "line exceeds 64 KiB".into(),
            });
            continue;
        }
        if scripts.len() >= MAX_RECORDS {
            warnings.push(ParseWarning {
                line: line_number,
                reason: "subscription exceeds 1,000 scripts".into(),
            });
            break;
        }
        if is_metadata_record(line) {
            continue;
        }
        let record = match serde_json::from_str::<JsonlScript>(line) {
            Ok(record) => record,
            Err(error) => {
                warnings.push(ParseWarning {
                    line: line_number,
                    reason: format!("invalid JSON: {error}"),
                });
                continue;
            }
        };
        match validate_record(record, subscription) {
            Ok(script) if ids.insert(script.id.clone()) => scripts.push(script),
            Ok(_) => warnings.push(ParseWarning {
                line: line_number,
                reason: "duplicate script id".into(),
            }),
            Err(reason) => warnings.push(ParseWarning {
                line: line_number,
                reason,
            }),
        }
    }
    (scripts, warnings)
}

fn parse_metadata(raw: &str) -> Option<JsonlMetadata> {
    raw.trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .find_map(|line| {
            let metadata = serde_json::from_str::<JsonlMetadata>(line).ok()?;
            (metadata.record_type == "metadata"
                && metadata.schema_version == 1
                && !metadata.name.trim().is_empty()
                && metadata.name.chars().count() <= 80)
                .then_some(metadata)
        })
}

fn is_metadata_record(line: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()
        .and_then(|record| record.get("type")?.as_str().map(|kind| kind == "metadata"))
        .unwrap_or(false)
}

fn validate_record(
    record: JsonlScript,
    subscription: &Subscription,
) -> Result<SubscriptionScript, String> {
    if record.schema_version != 1 {
        return Err("unsupported schemaVersion".into());
    }
    if record.id.len() > 64
        || record.id.is_empty()
        || !record.id.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || (byte == b'-' && index > 0)
        })
    {
        return Err("invalid id".into());
    }
    if record.name.trim().is_empty() || record.name.chars().count() > 80 {
        return Err("invalid name".into());
    }
    if record
        .description
        .as_ref()
        .is_some_and(|value| value.chars().count() > 500)
    {
        return Err("description exceeds 500 characters".into());
    }
    if record.tags.len() > 10
        || record
            .tags
            .iter()
            .any(|tag| tag.trim().is_empty() || tag.chars().count() > 32)
    {
        return Err("invalid tags".into());
    }
    if record.command.trim().is_empty() || record.command.len() > 32 * 1024 {
        return Err("invalid command".into());
    }
    if !matches!(record.risk_level.as_str(), "low" | "medium" | "high") {
        return Err("invalid riskLevel".into());
    }
    if record
        .platforms
        .iter()
        .any(|platform| !matches!(platform.as_str(), "linux" | "macos" | "windows"))
        || record.platforms.iter().collect::<HashSet<_>>().len() != record.platforms.len()
    {
        return Err("invalid platforms".into());
    }
    if record.version.trim().is_empty() || record.version.chars().count() > 64 {
        return Err("invalid version".into());
    }
    if let Some(homepage) = &record.homepage {
        if Url::parse(homepage)
            .ok()
            .filter(|url| url.scheme() == "https")
            .is_none()
        {
            return Err("homepage must be an HTTPS URL".into());
        }
    }
    Ok(SubscriptionScript {
        subscription_id: subscription.id.clone(),
        subscription_name: subscription.name.clone(),
        id: record.id,
        name: record.name,
        description: record.description,
        tags: record.tags,
        command: record.command,
        risk_level: record.risk_level,
        platforms: record.platforms,
        version: record.version,
        homepage: record.homepage,
    })
}

fn validate_subscription(name: &str, url: &str) -> Result<(), SubscriptionError> {
    if name.trim().is_empty() || name.chars().count() > 80 {
        return Err(SubscriptionError::Invalid(
            "name must contain 1 to 80 characters".into(),
        ));
    }
    let url =
        Url::parse(url.trim()).map_err(|_| SubscriptionError::Invalid("URL is invalid".into()))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(SubscriptionError::Invalid(
            "URL must be a credential-free HTTPS URL".into(),
        ));
    }
    Ok(())
}

fn official_subscription() -> Subscription {
    Subscription {
        id: OFFICIAL_ID.into(),
        name: OFFICIAL_NAME.into(),
        url: OFFICIAL_URL.into(),
        enabled: true,
        official: true,
        etag: None,
        last_fetched_at: None,
        last_success_at: None,
        last_error: None,
    }
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn io_error(error: std::io::Error) -> SubscriptionError {
    SubscriptionError::Storage(error.to_string())
}
fn write_atomic(path: &Path, content: &[u8]) -> Result<(), SubscriptionError> {
    let temporary = path.with_file_name(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| SubscriptionError::Storage("invalid filename".into()))?,
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        let mut file = File::create(&temporary).map_err(io_error)?;
        file.write_all(content).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        drop(file);
        fs::rename(&temporary, path).map_err(io_error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn subscription() -> Subscription {
        official_subscription()
    }
    #[test]
    fn parses_valid_records_and_skips_invalid_ones() {
        let raw = "\u{feff}# comment\n{\"schemaVersion\":1,\"id\":\"disk-usage\",\"name\":\"Disk\",\"command\":\"df -h\",\"riskLevel\":\"low\",\"version\":\"1\"}\n{\"schemaVersion\":1,\"id\":\"bad\",\"name\":\"Bad\",\"command\":\"\",\"riskLevel\":\"low\",\"version\":\"1\"}";
        let (scripts, warnings) = parse_jsonl(&subscription(), raw);
        assert_eq!(scripts.len(), 1);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn metadata_record_is_not_parsed_as_a_script() {
        let raw = "{\"schemaVersion\":1,\"type\":\"metadata\",\"name\":\"MJJ Scripts\"}\n{\"schemaVersion\":1,\"id\":\"disk-usage\",\"name\":\"Disk\",\"command\":\"df -h\",\"riskLevel\":\"low\",\"version\":\"1\"}";
        let (scripts, warnings) = parse_jsonl(&subscription(), raw);
        assert_eq!(scripts.len(), 1);
        assert!(warnings.is_empty());
        assert_eq!(parse_metadata(raw).unwrap().name, "MJJ Scripts");
    }
    #[test]
    fn rejects_private_addresses() {
        assert!(!is_public_address("127.0.0.1".parse().unwrap()));
        assert!(!is_public_address("10.0.0.1".parse().unwrap()));
        assert!(!is_public_address("::1".parse().unwrap()));
        assert!(is_public_address("8.8.8.8".parse().unwrap()));
    }
    #[test]
    fn rejects_subscription_over_record_limit() {
        let raw = (0..=MAX_RECORDS)
            .map(|index| format!(r#"{{"schemaVersion":1,"id":"script-{index}","name":"Script {index}","command":"echo ok","riskLevel":"low","version":"1"}}"#))
            .collect::<Vec<_>>()
            .join("\n");
        let (scripts, warnings) = parse_jsonl(&subscription(), &raw);
        assert_eq!(scripts.len(), MAX_RECORDS);
        assert!(warnings
            .iter()
            .any(|warning| warning.reason == "subscription exceeds 1,000 scripts"));
    }

    #[test]
    fn creates_official_config() {
        let directory =
            std::env::temp_dir().join(format!("mjj-ssh-subscriptions-{}", uuid::Uuid::new_v4()));
        let store = SubscriptionStore::new(&directory);
        let subscription = &store.list().unwrap()[0];
        assert_eq!(subscription.id, OFFICIAL_ID);
        assert_eq!(subscription.name, OFFICIAL_NAME);
        fs::remove_dir_all(directory).unwrap();
    }
}
