use app_lib::ai::client::{system_rules_for, AiClientError, OpenAiCompatibleClient};
use app_lib::ai::models::{AiChatMessage, AiMessageRole, ExecutionMode};
use app_lib::vault::AiProviderConfigSecret;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

const TEST_API_KEY: &str = "integration-test-key";

fn client_for(port: u16) -> OpenAiCompatibleClient {
    OpenAiCompatibleClient::from_config(AiProviderConfigSecret {
        base_url: format!("http://127.0.0.1:{port}/v1/"),
        api_key: TEST_API_KEY.into(),
        model: "mock-model".into(),
        timeout_seconds: 10,
    })
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0; 1024];
    let header_end;
    loop {
        let bytes_read = stream.read(&mut buffer).await.unwrap();
        assert!(bytes_read > 0, "mock server received an incomplete request");
        request.extend_from_slice(&buffer[..bytes_read]);
        if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = position + 4;
            break;
        }
    }

    let headers = String::from_utf8_lossy(&request[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.strip_prefix("content-length: ")
                .or_else(|| line.strip_prefix("Content-Length: "))
        })
        .unwrap()
        .parse::<usize>()
        .unwrap();
    while request.len() - header_end < content_length {
        let bytes_read = stream.read(&mut buffer).await.unwrap();
        assert!(
            bytes_read > 0,
            "mock server received an incomplete request body"
        );
        request.extend_from_slice(&buffer[..bytes_read]);
    }

    String::from_utf8(request).unwrap()
}

async fn start_mock_server(response: String, keep_open: bool) -> (u16, oneshot::Receiver<String>) {
    start_chunked_mock_server(vec![response], Duration::ZERO, keep_open).await
}

async fn start_chunked_mock_server(
    response_chunks: Vec<String>,
    delay_between_chunks: Duration,
    keep_open: bool,
) -> (u16, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (request_tx, request_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        let _ = request_tx.send(request);
        for response_chunk in response_chunks {
            stream.write_all(response_chunk.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
            if !delay_between_chunks.is_zero() {
                sleep(delay_between_chunks).await;
            }
        }
        if keep_open {
            sleep(Duration::from_secs(2)).await;
        }
    });
    (port, request_rx)
}

fn test_messages() -> [AiChatMessage; 1] {
    [AiChatMessage {
        role: AiMessageRole::User,
        content: "Test streamed response".into(),
    }]
}

#[tokio::test]
async fn sends_ordered_system_messages_and_streams_sse_deltas() {
    let response = concat!(
        "HTTP/1.1 200 OK\r\n",
        "content-type: text/event-stream\r\n",
        "connection: close\r\n\r\n",
        "data: {\"id\":\"mock\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"mock-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"first \"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"mock\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"mock-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"second\"},\"finish_reason\":null}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (port, request_rx) = start_mock_server(response.to_owned(), false).await;
    let client = client_for(port);
    let rules = system_rules_for(&ExecutionMode::ReadOnly);
    let agent_prompt = "## Agent\nUse concise Chinese.";
    let messages = [AiChatMessage {
        role: AiMessageRole::User,
        content: "Inspect disk usage".into(),
    }];
    let mut deltas = Vec::new();

    client
        .stream_chat(
            rules,
            agent_prompt,
            &messages,
            CancellationToken::new(),
            |delta| deltas.push(delta),
        )
        .await
        .unwrap();

    assert_eq!(deltas.concat(), "first second");
    let request = request_rx.await.unwrap();
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1"));
    assert!(
        headers.contains("authorization: Bearer integration-test-key")
            || headers.contains("Authorization: Bearer integration-test-key")
    );

    let body: Value = serde_json::from_str(body).unwrap();
    assert_eq!(body["model"], "mock-model");
    assert_eq!(body["stream"], true);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], rules);
    assert_eq!(body["messages"][1]["role"], "system");
    assert_eq!(body["messages"][1]["content"], agent_prompt);
    assert_eq!(body["messages"][2]["role"], "user");
    assert_eq!(body["messages"][2]["content"], "Inspect disk usage");
}

#[tokio::test]
async fn provider_http_errors_do_not_start_a_stream() {
    for status in [
        "401 Unauthorized",
        "429 Too Many Requests",
        "500 Internal Server Error",
    ] {
        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n{{\"error\":{{\"message\":\"mock failure\"}}}}"
        );
        let (port, request_rx) = start_mock_server(response, false).await;
        let mut deltas = Vec::new();
        let result = client_for(port)
            .stream_chat(
                system_rules_for(&ExecutionMode::ReadOnly),
                "## Agent\nTest.",
                &[AiChatMessage {
                    role: AiMessageRole::User,
                    content: "Test provider failure".into(),
                }],
                CancellationToken::new(),
                |delta| deltas.push(delta),
            )
            .await;

        let error = result.expect_err("HTTP failure must not start a successful stream");
        assert!(matches!(
            error,
            AiClientError::Provider(_) | AiClientError::Stream(_)
        ));
        assert!(!error.to_string().contains(TEST_API_KEY));
        assert!(deltas.is_empty());
        let _ = request_rx.await.unwrap();
    }
}

#[tokio::test]
async fn parses_an_sse_event_split_across_tcp_writes() {
    let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
    let event = "data: {\"id\":\"mock\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"mock-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"split event\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n";
    let split_at = event.len() / 2;
    let (port, request_rx) = start_chunked_mock_server(
        vec![
            format!("{headers}{}", &event[..split_at]),
            event[split_at..].to_owned(),
        ],
        Duration::from_millis(10),
        false,
    )
    .await;
    let mut deltas = Vec::new();

    client_for(port)
        .stream_chat(
            system_rules_for(&ExecutionMode::ReadOnly),
            "## Agent\nTest.",
            &test_messages(),
            CancellationToken::new(),
            |delta| deltas.push(delta),
        )
        .await
        .unwrap();

    assert_eq!(deltas, ["split event"]);
    let _ = request_rx.await.unwrap();
}

#[tokio::test]
async fn invalid_sse_payload_returns_an_error_without_output() {
    let response = concat!(
        "HTTP/1.1 200 OK\r\n",
        "content-type: text/event-stream\r\n",
        "connection: close\r\n\r\n",
        "data: {not-valid-json}\n\n"
    );
    let (port, request_rx) = start_mock_server(response.to_owned(), false).await;
    let mut deltas = Vec::new();
    let result = client_for(port)
        .stream_chat(
            system_rules_for(&ExecutionMode::ReadOnly),
            "## Agent\nTest.",
            &test_messages(),
            CancellationToken::new(),
            |delta| deltas.push(delta),
        )
        .await;

    assert!(matches!(result, Err(AiClientError::Stream(_))));
    assert!(deltas.is_empty());
    let _ = request_rx.await.unwrap();
}

#[tokio::test]
async fn preserves_deltas_when_the_server_disconnects_mid_stream() {
    let response = concat!(
        "HTTP/1.1 200 OK\r\n",
        "content-type: text/event-stream\r\n",
        "connection: close\r\n\r\n",
        "data: {\"id\":\"mock\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"mock-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\n"
    );
    let (port, request_rx) = start_mock_server(response.to_owned(), false).await;
    let mut deltas = Vec::new();
    let result = client_for(port)
        .stream_chat(
            system_rules_for(&ExecutionMode::ReadOnly),
            "## Agent\nTest.",
            &test_messages(),
            CancellationToken::new(),
            |delta| deltas.push(delta),
        )
        .await;

    assert!(matches!(result, Err(AiClientError::Stream(_))));
    assert_eq!(deltas, ["partial"]);
    let _ = request_rx.await.unwrap();
}

#[tokio::test]
async fn request_timeout_returns_an_error_without_output() {
    let (port, request_rx) = start_mock_server(String::new(), true).await;
    let config = AiProviderConfigSecret {
        base_url: format!("http://127.0.0.1:{port}/v1"),
        api_key: TEST_API_KEY.into(),
        model: "mock-model".into(),
        timeout_seconds: 10,
    };
    let mut deltas = Vec::new();
    let result =
        OpenAiCompatibleClient::from_config_with_timeout(config, Duration::from_millis(50))
            .stream_chat(
                system_rules_for(&ExecutionMode::ReadOnly),
                "## Agent\nTest.",
                &test_messages(),
                CancellationToken::new(),
                |delta| deltas.push(delta),
            )
            .await;

    let error = result.expect_err("timed out request must not complete successfully");
    assert!(matches!(
        error,
        AiClientError::Provider(_) | AiClientError::Stream(_)
    ));
    assert!(!error.to_string().contains(TEST_API_KEY));
    assert!(deltas.is_empty());
    let _ = request_rx.await.unwrap();
}

#[tokio::test]
async fn cancellation_stops_waiting_for_an_open_stream() {
    let response = concat!(
        "HTTP/1.1 200 OK\r\n",
        "content-type: text/event-stream\r\n",
        "connection: keep-alive\r\n\r\n"
    );
    let (port, request_rx) = start_mock_server(response.to_owned(), true).await;
    let client = client_for(port);
    let cancellation_token = CancellationToken::new();
    let task_token = cancellation_token.clone();
    let task = tokio::spawn(async move {
        client
            .stream_chat(
                system_rules_for(&ExecutionMode::ReadOnly),
                "## Agent\nTest.",
                &[AiChatMessage {
                    role: AiMessageRole::User,
                    content: "Wait for cancellation".into(),
                }],
                task_token,
                |_| {},
            )
            .await
    });

    let _ = request_rx.await.unwrap();
    cancellation_token.cancel();
    assert!(matches!(task.await.unwrap(), Err(AiClientError::Cancelled)));
}
