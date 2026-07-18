use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use yevune_core::MusicClient;

#[tokio::test]
async fn structured_lyrics_decode_opensubsonic_payload() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let bytes = socket.read(&mut request).await.unwrap();
            let line = std::str::from_utf8(&request[..bytes])
                .unwrap()
                .lines()
                .next()
                .unwrap();
            let data = if line.contains("/rest/getUser?") {
                ",\"user\":{\"username\":\"admin\",\"adminRole\":true}"
            } else if line.contains("/rest/getLyricsBySongId?") {
                assert!(line.contains("id=tr-1"));
                ",\"lyricsList\":{\"structuredLyrics\":[{\"displayArtist\":\"Band\",\"displayTitle\":\"Blue Sky\",\"lang\":\"zh\",\"offset\":120,\"synced\":true,\"line\":[{\"start\":1500,\"value\":\"第一句\"}]}]}"
            } else {
                ""
            };
            let body = format!(
                "{{\"subsonic-response\":{{\"status\":\"ok\",\"version\":\"1.16.1\",\"type\":\"yevune-server\",\"serverVersion\":\"0.1.0\",\"openSubsonic\":true{data}}}}}"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
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
    let lyrics = client
        .get_lyrics_by_song_id("tr-1".to_owned())
        .await
        .unwrap();
    server.await.unwrap();

    assert_eq!(lyrics.len(), 1);
    assert!(lyrics[0].synced);
    assert_eq!(lyrics[0].offset, 120);
    assert_eq!(lyrics[0].lines[0].start, Some(1500));
    assert_eq!(lyrics[0].lines[0].value, "第一句");
}
