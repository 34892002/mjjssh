use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use super::models::{content_hash, RemoteDocument};

pub const GIST_FILE_NAME: &str = "mjjssh-vault.json";
pub const SYNC_GIST_DESCRIPTION: &str = "MJJSSH encrypted cloud sync vault";
const GITHUB_API_BASE_URL: &str = "https://api.github.com";
const SYNC_LIST_PAGE_SIZE: usize = 100;

#[derive(Debug, thiserror::Error)]
pub enum GithubGistError {
    #[error("GitHub authentication failed")]
    Authentication,
    #[error("GitHub rate limit was reached")]
    RateLimited,
    #[error("GitHub Gist was not found")]
    NotFound,
    #[error("GitHub rejected the update because the remote changed")]
    Conflict,
    #[error("GitHub returned an invalid Gist response")]
    InvalidResponse,
    #[error("GitHub request failed: {0}")]
    Request(String),
}

pub struct GithubGistRemote {
    client: Client,
    api_base_url: String,
}

impl GithubGistRemote {
    pub fn new() -> Result<Self, GithubGistError> {
        Self::with_base_url(GITHUB_API_BASE_URL)
    }

    pub fn with_base_url(api_base_url: &str) -> Result<Self, GithubGistError> {
        Self::with_base_url_and_timeout(api_base_url, Duration::from_secs(20))
    }

    fn with_base_url_and_timeout(
        api_base_url: &str,
        timeout: Duration,
    ) -> Result<Self, GithubGistError> {
        let client = Client::builder()
            .timeout(timeout)
            .user_agent("MJJSSH cloud sync")
            .build()
            .map_err(|error| GithubGistError::Request(error.to_string()))?;
        Ok(Self {
            client,
            api_base_url: api_base_url.trim_end_matches('/').into(),
        })
    }

    pub async fn get(
        &self,
        token: &str,
        remote_id: &str,
    ) -> Result<RemoteDocument, GithubGistError> {
        let response = self
            .client
            .get(format!("{}/gists/{remote_id}", self.api_base_url))
            .bearer_auth(token)
            .send()
            .await
            .map_err(request_error)?;
        let gist: GistResponse = check_response(response)
            .await?
            .json()
            .await
            .map_err(|_| GithubGistError::InvalidResponse)?;
        document_from_gist(gist)
    }

    pub async fn find_sync_vaults(
        &self,
        token: &str,
    ) -> Result<Vec<RemoteDocument>, GithubGistError> {
        let response = self
            .client
            .get(format!("{}/gists", self.api_base_url))
            .bearer_auth(token)
            .query(&[("per_page", SYNC_LIST_PAGE_SIZE)])
            .send()
            .await
            .map_err(request_error)?;
        let summaries: Vec<GistSummary> = check_response(response)
            .await?
            .json()
            .await
            .map_err(|_| GithubGistError::InvalidResponse)?;
        let mut vaults = Vec::new();
        for summary in summaries
            .into_iter()
            .filter(|summary| summary.description.as_deref() == Some(SYNC_GIST_DESCRIPTION))
        {
            vaults.push(self.get(token, &summary.id).await?);
        }
        Ok(vaults)
    }

    pub async fn create(
        &self,
        token: &str,
        content: &str,
    ) -> Result<RemoteDocument, GithubGistError> {
        let response = self
            .client
            .post(format!("{}/gists", self.api_base_url))
            .bearer_auth(token)
            .json(&CreateGistRequest::new(content))
            .send()
            .await
            .map_err(request_error)?;
        let gist: GistResponse = check_response(response)
            .await?
            .json()
            .await
            .map_err(|_| GithubGistError::InvalidResponse)?;
        document_from_gist(gist)
    }

    pub async fn update(
        &self,
        token: &str,
        remote_id: &str,
        content: &str,
    ) -> Result<RemoteDocument, GithubGistError> {
        let response = self
            .client
            .patch(format!("{}/gists/{remote_id}", self.api_base_url))
            .bearer_auth(token)
            .json(&UpdateGistRequest::new(content))
            .send()
            .await
            .map_err(request_error)?;
        let gist: GistResponse = check_response(response)
            .await?
            .json()
            .await
            .map_err(|_| GithubGistError::InvalidResponse)?;
        document_from_gist(gist)
    }

    pub async fn delete(&self, token: &str, remote_id: &str) -> Result<(), GithubGistError> {
        let response = self
            .client
            .delete(format!("{}/gists/{remote_id}", self.api_base_url))
            .bearer_auth(token)
            .send()
            .await
            .map_err(request_error)?;
        check_response(response).await?;
        Ok(())
    }
}

#[derive(Deserialize)]
struct GistSummary {
    id: String,
    description: Option<String>,
}

#[derive(Deserialize)]
struct GistResponse {
    id: String,
    files: std::collections::HashMap<String, GistFile>,
}

#[derive(Deserialize)]
struct GistFile {
    content: Option<String>,
    truncated: Option<bool>,
}

#[derive(Serialize)]
struct CreateGistRequest {
    description: &'static str,
    public: bool,
    files: std::collections::HashMap<&'static str, GistFileContent>,
}

#[derive(Serialize)]
struct UpdateGistRequest {
    files: std::collections::HashMap<&'static str, GistFileContent>,
}

#[derive(Serialize)]
struct GistFileContent {
    content: String,
}

impl CreateGistRequest {
    fn new(content: &str) -> Self {
        Self {
            description: SYNC_GIST_DESCRIPTION,
            public: false,
            files: gist_files(content),
        }
    }
}

impl UpdateGistRequest {
    fn new(content: &str) -> Self {
        Self {
            files: gist_files(content),
        }
    }
}

fn gist_files(content: &str) -> std::collections::HashMap<&'static str, GistFileContent> {
    [(
        GIST_FILE_NAME,
        GistFileContent {
            content: content.into(),
        },
    )]
    .into_iter()
    .collect()
}

fn document_from_gist(gist: GistResponse) -> Result<RemoteDocument, GithubGistError> {
    let file = gist
        .files
        .get(GIST_FILE_NAME)
        .ok_or(GithubGistError::InvalidResponse)?;
    if file.truncated.unwrap_or(false) {
        return Err(GithubGistError::InvalidResponse);
    }
    let content = file
        .content
        .clone()
        .ok_or(GithubGistError::InvalidResponse)?;
    Ok(RemoteDocument {
        remote_id: gist.id,
        content_hash: content_hash(&content),
        content,
    })
}

async fn check_response(response: reqwest::Response) -> Result<reqwest::Response, GithubGistError> {
    match response.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(GithubGistError::Authentication),
        StatusCode::NOT_FOUND => Err(GithubGistError::NotFound),
        StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT => Err(GithubGistError::Conflict),
        StatusCode::TOO_MANY_REQUESTS => Err(GithubGistError::RateLimited),
        status if status.is_success() => Ok(response),
        status => {
            let detail = response
                .text()
                .await
                .ok()
                .map(|body| body.chars().take(512).collect::<String>())
                .filter(|body| !body.trim().is_empty())
                .unwrap_or_else(|| "no response body".into());
            Err(GithubGistError::Request(format!("HTTP {status}: {detail}")))
        }
    }
}

fn request_error(error: reqwest::Error) -> GithubGistError {
    if error.is_timeout() {
        GithubGistError::Request("request timed out".into())
    } else {
        GithubGistError::Request(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    use super::*;

    fn mock_server(
        status: u16,
        headers: &[(&str, &str)],
        body: &str,
    ) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_owned();
        let headers = headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<Vec<_>>();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 8192];
            let length = stream.read(&mut request).unwrap();
            let _ = sender.send(String::from_utf8_lossy(&request[..length]).into_owned());
            let mut response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            for (name, value) in headers {
                response.push_str(&format!("{name}: {value}\r\n"));
            }
            response.push_str("\r\n");
            response.push_str(&body);
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{address}"), receiver)
    }

    fn delayed_mock_server(delay: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            thread::sleep(delay);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
        });
        format!("http://{address}")
    }

    fn valid_gist(content: &str) -> String {
        format!(
            r#"{{"id":"gist-id","updated_at":"2026-07-20T12:00:00Z","files":{{"{GIST_FILE_NAME}":{{"content":"{content}","truncated":false}}}}}}"#
        )
    }

    #[tokio::test]
    async fn gets_gist_content() {
        let (base_url, request) = mock_server(200, &[], &valid_gist("ciphertext"));
        let remote = GithubGistRemote::with_base_url(&base_url).unwrap();
        let document = remote.get("secret-token", "gist-id").await.unwrap();
        let request = request.recv().unwrap();
        assert!(request.starts_with("GET /gists/gist-id HTTP/1.1"));
        assert!(request.contains("authorization: Bearer secret-token"));
        assert_eq!(document.remote_id, "gist-id");
        assert_eq!(document.content, "ciphertext");
    }

    #[tokio::test]
    async fn creates_a_private_gist_with_the_encrypted_file() {
        let (base_url, request) = mock_server(201, &[], &valid_gist("ciphertext"));
        let remote = GithubGistRemote::with_base_url(&base_url).unwrap();
        remote
            .create("secret-token", "encrypted-content")
            .await
            .unwrap();
        let request = request.recv().unwrap();
        assert!(request.starts_with("POST /gists HTTP/1.1"));
        assert!(request.contains("authorization: Bearer secret-token"));
        assert!(request.contains("\"public\":false"));
        assert!(request.contains(GIST_FILE_NAME));
        assert!(request.contains("encrypted-content"));
    }

    #[tokio::test]
    async fn classifies_remote_error_statuses() {
        for (status, expected) in [
            (401, GithubGistError::Authentication),
            (404, GithubGistError::NotFound),
            (409, GithubGistError::Conflict),
            (429, GithubGistError::RateLimited),
        ] {
            let (base_url, _) = mock_server(status, &[], "{}");
            let remote = GithubGistRemote::with_base_url(&base_url).unwrap();
            let error = remote.get("token", "gist-id").await.unwrap_err();
            assert_eq!(error.to_string(), expected.to_string());
        }
    }

    #[tokio::test]
    async fn classifies_timeouts_without_revealing_the_token() {
        let base_url = delayed_mock_server(Duration::from_millis(200));
        let remote =
            GithubGistRemote::with_base_url_and_timeout(&base_url, Duration::from_millis(20))
                .unwrap();
        let error = remote.get("secret-token", "gist-id").await.unwrap_err();
        assert_eq!(
            error.to_string(),
            "GitHub request failed: request timed out"
        );
        assert!(!error.to_string().contains("secret-token"));
    }

    #[tokio::test]
    async fn rejects_missing_or_truncated_sync_file() {
        let missing = r#"{"id":"gist-id","files":{}}"#;
        let (base_url, _) = mock_server(200, &[], missing);
        let remote = GithubGistRemote::with_base_url(&base_url).unwrap();
        assert!(matches!(
            remote.get("token", "gist-id").await,
            Err(GithubGistError::InvalidResponse)
        ));

        let truncated =
            format!(r#"{{"id":"gist-id","files":{{"{GIST_FILE_NAME}":{{"truncated":true}}}}}}"#);
        let (base_url, _) = mock_server(200, &[], &truncated);
        let remote = GithubGistRemote::with_base_url(&base_url).unwrap();
        assert!(matches!(
            remote.get("token", "gist-id").await,
            Err(GithubGistError::InvalidResponse)
        ));
    }

    #[tokio::test]
    async fn updates_the_expected_gist_without_conditional_headers() {
        let (base_url, request) = mock_server(200, &[], &valid_gist("updated-ciphertext"));
        let remote = GithubGistRemote::with_base_url(&base_url).unwrap();
        let document = remote
            .update("token", "gist-id", "encrypted")
            .await
            .unwrap();
        let request = request.recv().unwrap();
        assert!(request.starts_with("PATCH /gists/gist-id HTTP/1.1"));
        assert!(!request.to_lowercase().contains("if-match:"));
        assert!(request.contains("encrypted"));
        assert_eq!(document.content, "updated-ciphertext");
    }
}
