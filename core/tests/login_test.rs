use std::sync::Arc;

use yevune_core::MusicClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[tokio::test]
async fn login_pings_with_subsonic_credentials() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let paths = Arc::new(Mutex::new(Vec::new()));
    let expected_paths = paths.clone();

    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let bytes = socket.read(&mut request).await.unwrap();
            let first_line = std::str::from_utf8(&request[..bytes])
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .to_owned();
            expected_paths.lock().await.push(first_line);
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 123\r\nconnection: close\r\n\r\n{\"subsonic-response\":{\"status\":\"ok\",\"version\":\"1.16.1\",\"type\":\"yevune-server\",\"serverVersion\":\"0.1.0\",\"openSubsonic\":true}}",
                )
                .await
                .unwrap();
        }
    });

    let client = MusicClient::new();
    client
        .login(
            format!("http://{address}"),
            "admin".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap();
    client.ping().await.unwrap();
    server.await.unwrap();

    let paths = paths.lock().await;
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|path| path.contains("u=admin")));
    assert!(paths.iter().all(|path| path.contains("p=secret")));
    assert!(paths.iter().all(|path| path.contains("v=1.16.1")));
    assert!(paths.iter().all(|path| path.contains("c=music-mac")));
    assert!(paths.iter().all(|path| path.contains("f=json")));
    assert!(paths.iter().all(|path| path.contains("/rest/ping?")));
}
