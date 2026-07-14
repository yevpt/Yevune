use std::sync::Arc;

use contract::{Principal, PrincipalType, ScopeType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use yevune_core::{CoreError, MusicClient};

async fn mock_server(
    bodies: Vec<String>,
) -> (
    std::net::SocketAddr,
    Arc<Mutex<Vec<String>>>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let handle = tokio::spawn(async move {
        for body in bodies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let count = socket.read(&mut bytes).await.unwrap();
            observed
                .lock()
                .await
                .push(String::from_utf8_lossy(&bytes[..count]).into_owned());
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(body.as_bytes()).await.unwrap();
        }
    });
    (address, requests, handle)
}

fn ok(inner: &str) -> String {
    format!(
        "{{\"subsonic-response\":{{\"status\":\"ok\",\"version\":\"1.16.1\",\"type\":\"yevune-server\",\"serverVersion\":\"0.1.0\",\"openSubsonic\":true{}}}}}",
        if inner.is_empty() {
            String::new()
        } else {
            format!(",{inner}")
        }
    )
}

fn current_user() -> String {
    ok("\"user\":{\"username\":\"admin\",\"adminRole\":true}")
}

fn failed(code: u32, message: &str) -> String {
    format!(
        "{{\"subsonic-response\":{{\"status\":\"failed\",\"version\":\"1.16.1\",\"error\":{{\"code\":{code},\"message\":\"{message}\"}}}}}}"
    )
}

async fn logged_in(address: std::net::SocketAddr) -> Arc<MusicClient> {
    let client = MusicClient::new();
    client
        .login(format!("http://{address}"), "admin".into(), "secret".into())
        .await
        .unwrap();
    client
}

#[tokio::test]
async fn access_rule_operations_decode_contract_types_and_encode_ordered_parameters() {
    let listed = "\"accessRules\":{\"accessRule\":[{\"id\":\"ru-3\",\"scopeType\":\"genre\",\"scopeId\":\"摇滚\",\"scopeName\":\"摇滚\",\"grants\":[{\"type\":\"user\",\"id\":\"us-2\"}]}]}";
    let saved = "\"accessRule\":{\"id\":\"ru-3\",\"scopeType\":\"genre\",\"scopeId\":\"摇滚 & Blues\",\"scopeName\":\"摇滚 & Blues\",\"grants\":[{\"type\":\"user\",\"id\":\"us-2\"},{\"type\":\"role\",\"id\":\"ro-7\"}]}";
    let empty = "\"accessRule\":{\"id\":\"ru-4\",\"scopeType\":\"track\",\"scopeId\":\"tr-9\",\"scopeName\":null,\"grants\":[]}";
    let (address, requests, handle) = mock_server(vec![
        ok(""),
        current_user(),
        ok(listed),
        ok(saved),
        ok(empty),
        ok(""),
    ])
    .await;
    let client = logged_in(address).await;

    let rules = client.list_access_rules().await.unwrap();
    assert_eq!(rules[0].scope_name.as_deref(), Some("摇滚"));
    assert_eq!(rules[0].grants[0].id, "us-2");

    let saved = client
        .set_access_rule(
            ScopeType::Genre,
            "摇滚 & Blues".into(),
            vec![
                Principal {
                    principal_type: PrincipalType::User,
                    id: "us-2".into(),
                },
                Principal {
                    principal_type: PrincipalType::Role,
                    id: "ro-7".into(),
                },
            ],
        )
        .await
        .unwrap();
    assert_eq!(saved.scope_type, ScopeType::Genre);
    client
        .set_access_rule(ScopeType::Track, "tr-9".into(), vec![])
        .await
        .unwrap();
    client.delete_access_rule("ru-3".into()).await.unwrap();
    handle.await.unwrap();

    let requests = requests.lock().await;
    assert!(requests[2].contains("/rest/ext/getAccessRules?"));
    assert!(requests[3].contains("/rest/ext/setAccessRule?"));
    assert!(requests[3].contains("scopeType=genre"));
    assert!(requests[3].contains("scopeId=%E6%91%87%E6%BB%9A+%26+Blues"));
    let user_grant = requests[3].find("grant=user%3Aus-2").unwrap();
    let role_grant = requests[3].find("grant=role%3Aro-7").unwrap();
    assert!(user_grant < role_grant);
    assert!(requests[4].contains("scopeType=track"));
    assert!(!requests[4].contains("grant="));
    assert!(requests[5].contains("/rest/ext/deleteAccessRule?"));
    assert!(requests[5].contains("id=ru-3"));
}

#[tokio::test]
async fn access_rule_failure_envelope_maps_to_typed_server_error() {
    let (address, _, handle) =
        mock_server(vec![ok(""), current_user(), failed(50, "Admin only")]).await;
    let client = logged_in(address).await;

    let error = client.list_access_rules().await.unwrap_err();
    handle.await.unwrap();

    assert!(matches!(
        error,
        CoreError::Server { code: 50, message } if message == "Admin only"
    ));
}
