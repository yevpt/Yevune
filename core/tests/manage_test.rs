use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use yevune_core::{MusicClient, TagUpdate};

#[tokio::test]
async fn update_tags_sends_only_requested_override_fields() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 2048];
            let count = socket.read(&mut bytes).await.unwrap();
            observed
                .lock()
                .await
                .push(String::from_utf8_lossy(&bytes[..count]).into_owned());
            let request = String::from_utf8_lossy(&bytes[..count]);
            let body = if request.contains("/rest/getUser?") {
                "{\"subsonic-response\":{\"status\":\"ok\",\"version\":\"1.16.1\",\"type\":\"yevune-server\",\"serverVersion\":\"0.1.0\",\"openSubsonic\":true,\"user\":{\"username\":\"admin\",\"adminRole\":true}}}"
            } else {
                "{\"subsonic-response\":{\"status\":\"ok\",\"version\":\"1.16.1\",\"type\":\"yevune-server\",\"serverVersion\":\"0.1.0\",\"openSubsonic\":true}}"
            };
            socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });

    let client = MusicClient::new();
    client
        .login(format!("http://{address}"), "admin".into(), "secret".into())
        .await
        .unwrap();
    client
        .update_tags(
            "tr-1".into(),
            TagUpdate {
                title: Some("New Title".into()),
                album: None,
                artist: Some("New Artist".into()),
                genre: None,
                year: Some(2024),
                track: None,
                disc_number: None,
            },
        )
        .await
        .unwrap();
    server.await.unwrap();

    let update = requests.lock().await[2].clone();
    assert!(update.contains("/rest/ext/updateTags?"));
    assert!(update.contains("id=tr-1"));
    assert!(update.contains("title=New+Title"));
    assert!(update.contains("artist=New+Artist"));
    assert!(update.contains("year=2024"));
    assert!(!update.contains("genre="));
}
