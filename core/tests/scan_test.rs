use music_core::MusicClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn scan_operations_require_login() {
    let client = MusicClient::new();
    assert!(client.start_scan().await.is_err());
    assert!(client.scan_status().await.is_err());
}

#[tokio::test]
async fn detailed_prefix_scan_decodes_changes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for request_index in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let count = socket.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..count]);
            if request_index == 1 {
                assert!(request.contains("/rest/ext/startScan?"));
                assert!(request.contains("prefix=library%2F"));
            }
            let data = if request_index == 0 {
                ""
            } else {
                ",\"scanResult\":{\"added\":1,\"updated\":0,\"deleted\":0,\"unchanged\":2,\"changesTruncated\":false,\"changes\":[{\"action\":\"added\",\"objectKey\":\"library/song.flac\",\"track\":{\"id\":\"tr-1\",\"title\":\"Song\",\"size\":42,\"duration\":120,\"bitRate\":320}}]}"
            };
            let body = format!("{{\"subsonic-response\":{{\"status\":\"ok\"{data}}}}}");
            socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    let client = MusicClient::new();
    client
        .login(format!("http://{address}"), "admin".into(), "secret".into())
        .await
        .unwrap();
    let result = client.scan_prefix("library/".into()).await.unwrap();
    assert_eq!(result.added, 1);
    assert_eq!(result.changes[0].object_key, "library/song.flac");
    assert_eq!(result.changes[0].track.title, "Song");
    server.await.unwrap();
}
