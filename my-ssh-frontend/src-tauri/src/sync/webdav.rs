use std::time::Duration;

use reqwest::{Client, StatusCode, Url};

use super::models::{content_hash, RemoteDocument};

pub const WEBDAV_SYNC_FILE_NAME: &str = "mjjssh-vault.json";

#[derive(Debug, Clone)]
pub struct WebDavCredentials {
    pub url: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, thiserror::Error)]
pub enum WebDavError {
    #[error("WebDAV URL must use HTTPS and must not contain embedded credentials")]
    InvalidUrl,
    #[error("WebDAV authentication failed")]
    Authentication,
    #[error("WebDAV sync file was not found")]
    NotFound,
    #[error("WebDAV rejected the update because the remote changed")]
    Conflict,
    #[error("WebDAV server did not provide an ETag for the sync file")]
    MissingEtag,
    #[error("WebDAV request failed: {0}")]
    Request(String),
}

pub struct WebDavRemote {
    client: Client,
}

impl WebDavRemote {
    pub fn new() -> Result<Self, WebDavError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("MJJSSH cloud sync")
            .build()
            .map_err(|error| WebDavError::Request(error.to_string()))?;
        Ok(Self { client })
    }

    pub async fn get(
        &self,
        credentials: &WebDavCredentials,
    ) -> Result<RemoteDocument, WebDavError> {
        let url = validate_url(&credentials.url)?;
        let response = self
            .client
            .get(url.clone())
            .basic_auth(&credentials.username, Some(&credentials.password))
            .send()
            .await
            .map_err(request_error)?;
        let response = check_response(response).await?;
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .ok_or(WebDavError::MissingEtag)?;
        let content = response.text().await.map_err(request_error)?;
        Ok(RemoteDocument {
            remote_id: url.into(),
            content_hash: content_hash(&content),
            content,
            remote_updated_at: chrono::Utc::now().to_rfc3339(),
            etag: Some(etag),
        })
    }

    pub async fn find_sync_vaults(
        &self,
        credentials: &WebDavCredentials,
    ) -> Result<Vec<RemoteDocument>, WebDavError> {
        match self.get(credentials).await {
            Ok(document) => Ok(vec![document]),
            Err(WebDavError::NotFound) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    pub async fn create(
        &self,
        credentials: &WebDavCredentials,
        content: &str,
    ) -> Result<RemoteDocument, WebDavError> {
        let url = validate_url(&credentials.url)?;
        let response = self
            .client
            .put(url)
            .basic_auth(&credentials.username, Some(&credentials.password))
            .header(reqwest::header::IF_NONE_MATCH, "*")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(content.to_owned())
            .send()
            .await
            .map_err(request_error)?;
        check_response(response).await?;
        self.get(credentials).await
    }

    pub async fn update(
        &self,
        credentials: &WebDavCredentials,
        current: &RemoteDocument,
        content: &str,
    ) -> Result<RemoteDocument, WebDavError> {
        let etag = current.etag.as_deref().ok_or(WebDavError::MissingEtag)?;
        let url = validate_url(&credentials.url)?;
        let response = self
            .client
            .put(url)
            .basic_auth(&credentials.username, Some(&credentials.password))
            .header(reqwest::header::IF_MATCH, etag)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(content.to_owned())
            .send()
            .await
            .map_err(request_error)?;
        check_response(response).await?;
        self.get(credentials).await
    }

    pub async fn delete(&self, credentials: &WebDavCredentials) -> Result<(), WebDavError> {
        let url = validate_url(&credentials.url)?;
        let response = self
            .client
            .delete(url)
            .basic_auth(&credentials.username, Some(&credentials.password))
            .send()
            .await
            .map_err(request_error)?;
        check_response(response).await?;
        Ok(())
    }
}

pub fn validate_url(value: &str) -> Result<String, WebDavError> {
    let mut url = Url::parse(value.trim()).map_err(|_| WebDavError::InvalidUrl)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(WebDavError::InvalidUrl);
    }

    if !url.path().ends_with(".json") {
        url.path_segments_mut()
            .map_err(|_| WebDavError::InvalidUrl)?
            .pop_if_empty()
            .push(WEBDAV_SYNC_FILE_NAME);
    }

    Ok(url.into())
}

async fn check_response(response: reqwest::Response) -> Result<reqwest::Response, WebDavError> {
    match response.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(WebDavError::Authentication),
        StatusCode::NOT_FOUND => Err(WebDavError::NotFound),
        StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT => Err(WebDavError::Conflict),
        status if status.is_success() => Ok(response),
        status => Err(WebDavError::Request(format!("HTTP {status}"))),
    }
}

fn request_error(error: reqwest::Error) -> WebDavError {
    if error.is_timeout() {
        WebDavError::Request("request timed out".into())
    } else {
        WebDavError::Request("network request failed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_file_url() {
        assert_eq!(
            validate_url("https://dav.example.test/remote.php/dav/files/user/mjjssh.json").unwrap(),
            "https://dav.example.test/remote.php/dav/files/user/mjjssh.json"
        );
    }

    #[test]
    fn appends_the_sync_file_name_to_a_directory_url() {
        assert_eq!(
            validate_url("https://us2.workspace.org/webdav/").unwrap(),
            "https://us2.workspace.org/webdav/mjjssh-vault.json"
        );
        assert_eq!(
            validate_url("https://us2.workspace.org/webdav").unwrap(),
            "https://us2.workspace.org/webdav/mjjssh-vault.json"
        );
    }

    #[test]
    fn appends_the_sync_file_name_without_double_encoding_directory_segments() {
        assert_eq!(
            validate_url("https://dav.jianguoyun.com/dav/%E6%88%91%E7%9A%84%E5%9D%9A%E6%9E%9C%E4%BA%91/").unwrap(),
            "https://dav.jianguoyun.com/dav/%E6%88%91%E7%9A%84%E5%9D%9A%E6%9E%9C%E4%BA%91/mjjssh-vault.json"
        );
    }

    #[test]
    fn rejects_insecure_or_credentialed_url() {
        assert!(matches!(
            validate_url("http://dav.example.test/vault.json"),
            Err(WebDavError::InvalidUrl)
        ));
        assert!(matches!(
            validate_url("https://user:password@dav.example.test/vault.json"),
            Err(WebDavError::InvalidUrl)
        ));
        assert!(matches!(
            validate_url("https://dav.example.test/vault.json?token=secret"),
            Err(WebDavError::InvalidUrl)
        ));
    }
}
