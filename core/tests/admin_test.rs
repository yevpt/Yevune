use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use yevune_core::MusicClient;

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

fn current_user(admin: bool) -> String {
    ok(&format!(
        "\"user\":{{\"username\":\"admin\",\"adminRole\":{admin}}}"
    ))
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
async fn list_users_and_roles_decode_shared_contract_records() {
    let users = "\"users\":{\"user\":[{\"id\":\"us-1\",\"name\":\"admin\",\"email\":\"a@example.com\",\"created\":null,\"admin\":true,\"roles\":[\"admin\"]}]}";
    let roles = "\"roles\":{\"role\":[{\"id\":\"ro-1\",\"name\":\"admin\",\"isBuiltin\":true}]}";
    let (address, requests, handle) =
        mock_server(vec![ok(""), current_user(true), ok(users), ok(roles)]).await;
    let client = logged_in(address).await;

    let decoded_users = client.list_users().await.unwrap();
    let decoded_roles = client.list_roles().await.unwrap();
    handle.await.unwrap();

    assert_eq!(decoded_users[0].id, "us-1");
    assert_eq!(decoded_users[0].roles, vec!["admin"]);
    assert_eq!(decoded_roles[0].id, "ro-1");
    assert!(decoded_roles[0].is_builtin);
    let requests = requests.lock().await;
    assert!(requests[2].contains("/rest/ext/getUsers?"));
    assert!(requests[3].contains("/rest/ext/getRoles?"));
}

#[tokio::test]
async fn write_operations_encode_all_parameters() {
    let created_role = "\"role\":{\"id\":\"ro-9\",\"name\":\"孩子\",\"isBuiltin\":false}";
    let (address, requests, handle) = mock_server(vec![
        ok(""),
        current_user(true),
        ok(""),
        ok(""),
        ok(""),
        ok(created_role),
        ok(""),
        ok(""),
        ok(""),
        ok(""),
    ])
    .await;
    let client = logged_in(address).await;

    client
        .create_user("小明".into(), "m@example.com".into(), "s e&c".into(), false)
        .await
        .unwrap();
    client
        .update_user("小明".into(), "new@example.com".into(), true)
        .await
        .unwrap();
    client
        .change_password("小明".into(), "new secret".into())
        .await
        .unwrap();
    let role = client.create_role("孩子".into()).await.unwrap();
    client
        .assign_role("us-2".into(), role.id.clone())
        .await
        .unwrap();
    client
        .unassign_role("us-2".into(), role.id.clone())
        .await
        .unwrap();
    client.delete_role(role.id).await.unwrap();
    client.delete_user("小明".into()).await.unwrap();
    handle.await.unwrap();

    let requests = requests.lock().await;
    assert!(requests[2].contains("/rest/createUser?"));
    assert!(requests[2].contains("username=%E5%B0%8F%E6%98%8E"));
    assert!(requests[2].contains("email=m%40example.com"));
    assert!(requests[2].contains("password=s+e%26c"));
    assert!(requests[2].contains("adminRole=false"));
    assert!(requests[3].contains("/rest/updateUser?"));
    assert!(requests[3].contains("email=new%40example.com"));
    assert!(requests[3].contains("adminRole=true"));
    assert!(requests[4].contains("/rest/changePassword?"));
    assert!(requests[4].contains("password=new+secret"));
    assert!(requests[5].contains("/rest/ext/createRole?"));
    assert!(requests[5].contains("name=%E5%AD%A9%E5%AD%90"));
    assert!(requests[6].contains("/rest/ext/assignRole?"));
    assert!(requests[6].contains("userId=us-2"));
    assert!(requests[6].contains("roleId=ro-9"));
    assert!(requests[7].contains("/rest/ext/unassignRole?"));
    assert!(requests[8].contains("/rest/ext/deleteRole?"));
    assert!(requests[8].contains("id=ro-9"));
    assert!(requests[9].contains("/rest/deleteUser?"));
}
