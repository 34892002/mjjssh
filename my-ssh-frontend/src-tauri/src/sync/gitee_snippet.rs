use std::collections::HashMap;
use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use super::github_gist::{GIST_FILE_NAME, SYNC_GIST_DESCRIPTION};
use super::models::{content_hash, RemoteDocument};

const GITEE_API_BASE_URL: &str = "https://gitee.com/api/v5";

#[derive(Debug, thiserror::Error)]
pub enum GiteeSnippetError {
    #[error("Gitee authentication failed")]
    Authentication,
    #[error("Gitee rate limit was reached")]
    RateLimited,
    #[error("Gitee snippet was not found")]
    NotFound,
    #[error("Gitee returned an invalid snippet response")]
    InvalidResponse,
    #[error("Gitee request failed: {0}")]
    Request(String),
}

pub struct GiteeSnippetRemote {
    client: Client,
    api_base_url: String,
}

impl GiteeSnippetRemote {
    pub fn new() -> Result<Self, GiteeSnippetError> {
        Self::with_base_url(GITEE_API_BASE_URL)
    }

    pub fn with_base_url(api_base_url: &str) -> Result<Self, GiteeSnippetError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("MJJSSH cloud sync")
            .build()
            .map_err(|_| GiteeSnippetError::Request("could not create HTTP client".into()))?;
        Ok(Self {
            client,
            api_base_url: api_base_url.trim_end_matches('/').into(),
        })
    }

    pub async fn get(
        &self,
        token: &str,
        remote_id: &str,
    ) -> Result<RemoteDocument, GiteeSnippetError> {
        let response = self
            .client
            .get(format!("{}/gists/{remote_id}", self.api_base_url))
            .query(&[("access_token", token)])
            .send()
            .await
            .map_err(request_error)?;
        let response = check_response(response).await?;
        let gist: GiteeGistResponse = response
            .json()
            .await
            .map_err(|_| GiteeSnippetError::InvalidResponse)?;
        document_from_gist(gist)
    }

    pub async fn find_sync_vaults(
        &self,
        token: &str,
    ) -> Result<Vec<RemoteDocument>, GiteeSnippetError> {
        let response = self
            .client
            .get(format!("{}/gists", self.api_base_url))
            .query(&[("access_token", token), ("page", "1"), ("per_page", "100")])
            .send()
            .await
            .map_err(request_error)?;
        let summaries: Vec<GiteeGistSummary> = check_response(response)
            .await?
            .json()
            .await
            .map_err(|_| GiteeSnippetError::InvalidResponse)?;
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
    ) -> Result<RemoteDocument, GiteeSnippetError> {
        let response = self
            .client
            .post(format!("{}/gists", self.api_base_url))
            .json(&CreateGiteeGistRequest::new(token, content))
            .send()
            .await
            .map_err(request_error)?;
        let response = check_response(response).await?;
        let gist: GiteeGistResponse = response
            .json()
            .await
            .map_err(|_| GiteeSnippetError::InvalidResponse)?;
        document_from_gist(gist)
    }

    pub async fn update(
        &self,
        token: &str,
        remote_id: &str,
        content: &str,
    ) -> Result<RemoteDocument, GiteeSnippetError> {
        let response = self
            .client
            .patch(format!("{}/gists/{remote_id}", self.api_base_url))
            .json(&UpdateGiteeGistRequest::new(token, content))
            .send()
            .await
            .map_err(request_error)?;
        let response = check_response(response).await?;
        let gist: GiteeGistResponse = response
            .json()
            .await
            .map_err(|_| GiteeSnippetError::InvalidResponse)?;
        document_from_gist(gist)
    }

    pub async fn delete(&self, token: &str, remote_id: &str) -> Result<(), GiteeSnippetError> {
        let response = self
            .client
            .delete(format!("{}/gists/{remote_id}", self.api_base_url))
            .query(&[("access_token", token)])
            .send()
            .await
            .map_err(request_error)?;
        check_response(response).await?;
        Ok(())
    }
}

#[derive(Deserialize)]
struct GiteeGistSummary {
    id: String,
    description: Option<String>,
}

#[derive(Deserialize)]
struct GiteeGistResponse {
    id: String,
    updated_at: Option<String>,
    files: HashMap<String, GiteeGistFile>,
}

#[derive(Deserialize)]
struct GiteeGistFile {
    content: Option<String>,
}

#[derive(Serialize)]
struct CreateGiteeGistRequest {
    access_token: String,
    description: &'static str,
    public: bool,
    files: HashMap<&'static str, GiteeGistFileContent>,
}

#[derive(Serialize)]
struct UpdateGiteeGistRequest {
    access_token: String,
    files: HashMap<&'static str, GiteeGistFileContent>,
}

#[derive(Serialize)]
struct GiteeGistFileContent {
    content: String,
}

impl CreateGiteeGistRequest {
    fn new(token: &str, content: &str) -> Self {
        Self {
            access_token: token.into(),
            description: SYNC_GIST_DESCRIPTION,
            public: false,
            files: gist_files(content),
        }
    }
}

impl UpdateGiteeGistRequest {
    fn new(token: &str, content: &str) -> Self {
        Self {
            access_token: token.into(),
            files: gist_files(content),
        }
    }
}

fn gist_files(content: &str) -> HashMap<&'static str, GiteeGistFileContent> {
    [(
        GIST_FILE_NAME,
        GiteeGistFileContent {
            content: content.into(),
        },
    )]
    .into_iter()
    .collect()
}

fn document_from_gist(gist: GiteeGistResponse) -> Result<RemoteDocument, GiteeSnippetError> {
    let content = gist
        .files
        .get(GIST_FILE_NAME)
        .and_then(|file| file.content.clone())
        .ok_or(GiteeSnippetError::InvalidResponse)?;
    Ok(RemoteDocument {
        remote_id: gist.id,
        content_hash: content_hash(&content),
        content,
        revision: None,
        updated_at: gist.updated_at,
    })
}

async fn check_response(
    response: reqwest::Response,
) -> Result<reqwest::Response, GiteeSnippetError> {
    match response.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(GiteeSnippetError::Authentication),
        StatusCode::NOT_FOUND => Err(GiteeSnippetError::NotFound),
        StatusCode::TOO_MANY_REQUESTS => Err(GiteeSnippetError::RateLimited),
        status if status.is_success() => Ok(response),
        status => Err(GiteeSnippetError::Request(format!("HTTP {status}"))),
    }
}

fn request_error(error: reqwest::Error) -> GiteeSnippetError {
    if error.is_timeout() {
        GiteeSnippetError::Request("request timed out".into())
    } else {
        GiteeSnippetError::Request("network request failed".into())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    use super::*;

    fn mock_server(status: u16, body: &str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_owned();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 8192];
            let length = stream.read(&mut request).unwrap();
            let _ = sender.send(String::from_utf8_lossy(&request[..length]).into_owned());
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{address}"), receiver)
    }

    fn valid_gist(content: &str) -> String {
        format!(r#"{{"id":"snippet-id","files":{{"{GIST_FILE_NAME}":{{"content":"{content}"}}}}}}"#)
    }

    #[tokio::test]
    async fn creates_private_snippet_without_putting_token_in_url() {
        let (base_url, request) = mock_server(201, &valid_gist("ciphertext"));
        let remote = GiteeSnippetRemote::with_base_url(&base_url).unwrap();
        remote
            .create("secret-token", "encrypted-content")
            .await
            .unwrap();
        let request = request.recv().unwrap();
        assert!(request.starts_with("POST /gists HTTP/1.1"));
        assert!(!request.starts_with("POST /gists?access_token="));
        assert!(request.contains("\"access_token\":\"secret-token\""));
        assert!(request.contains("\"public\":false"));
        assert!(request.contains(GIST_FILE_NAME));
    }

    #[tokio::test]
    async fn gets_gist_and_keeps_token_out_of_error_messages() {
        let (base_url, request) = mock_server(200, &valid_gist("ciphertext"));
        let remote = GiteeSnippetRemote::with_base_url(&base_url).unwrap();
        let document = remote.get("secret-token", "snippet-id").await.unwrap();
        let request = request.recv().unwrap();
        assert!(request.starts_with("GET /gists/snippet-id?access_token=secret-token HTTP/1.1"));
        assert_eq!(document.content, "ciphertext");
    }

    #[tokio::test]
    async fn classifies_authentication_and_missing_snippets() {
        for (status, expected) in [(401, "authentication"), (404, "not found")] {
            let (base_url, _) = mock_server(status, "{}");
            let remote = GiteeSnippetRemote::with_base_url(&base_url).unwrap();
            let error = remote.get("secret-token", "snippet-id").await.unwrap_err();
            assert!(error.to_string().to_lowercase().contains(expected));
            assert!(!error.to_string().contains("secret-token"));
        }
    }
}
