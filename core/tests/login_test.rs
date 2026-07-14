use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use yevune_core::{CoreError, MusicClient};

#[tokio::test]
async fn login_pings_with_subsonic_credentials() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let paths = Arc::new(Mutex::new(Vec::new()));
    let expected_paths = paths.clone();

    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let bytes = socket.read(&mut request).await.unwrap();
            let first_line = std::str::from_utf8(&request[..bytes])
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .to_owned();
            let body = if first_line.contains("/rest/getUser?") {
                r#"{"subsonic-response":{"status":"ok","version":"1.16.1","type":"yevune-server","serverVersion":"0.1.0","openSubsonic":true,"user":{"username":"admin","adminRole":true}}}"#
            } else {
                r#"{"subsonic-response":{"status":"ok","version":"1.16.1","type":"yevune-server","serverVersion":"0.1.0","openSubsonic":true}}"#
            };
            expected_paths.lock().await.push(first_line);
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(body.as_bytes()).await.unwrap();
        }
    });

    let client = MusicClient::new();
    let session = client
        .login(
            format!("http://{address}"),
            "admin".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap();
    assert!(session.admin);
    client.ping().await.unwrap();
    server.await.unwrap();

    let paths = paths.lock().await;
    assert_eq!(paths.len(), 3);
    assert!(paths.iter().all(|path| path.contains("u=admin")));
    assert!(paths.iter().all(|path| path.contains("p=secret")));
    assert!(paths.iter().all(|path| path.contains("v=1.16.1")));
    assert!(paths.iter().all(|path| path.contains("c=music-mac")));
    assert!(paths.iter().all(|path| path.contains("f=json")));
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.contains("/rest/ping?"))
            .count(),
        2
    );
    assert!(paths
        .iter()
        .any(|path| { path.contains("/rest/getUser?") && path.contains("username=admin") }));
}

#[tokio::test]
async fn login_does_not_save_session_when_current_user_lookup_fails() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let bodies = [
            r#"{"subsonic-response":{"status":"ok","version":"1.16.1"}}"#,
            r#"{"subsonic-response":{"status":"failed","version":"1.16.1","error":{"code":50,"message":"Denied"}}}"#,
        ];
        for body in bodies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let count = socket.read(&mut request).await.unwrap();
            assert!(count > 0);
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(body.as_bytes()).await.unwrap();
        }
    });

    let client = MusicClient::new();
    let error = client
        .login(
            format!("http://{address}"),
            "admin".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap_err();
    server.await.unwrap();

    assert!(matches!(error, CoreError::Server { code: 50, .. }));
    assert!(matches!(
        client.ping().await,
        Err(CoreError::NotAuthenticated)
    ));
}
