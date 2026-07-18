use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use yevune_core::MusicClient;

#[tokio::test]
async fn get_starred_decodes_all_media_kinds_from_the_standard_endpoint() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let count = socket.read(&mut request).await.unwrap();
            let line = std::str::from_utf8(&request[..count])
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .to_owned();
            observed.lock().await.push(line.clone());
            let data = if line.contains("/rest/getUser?") {
                r#","user":{"adminRole":false}"#.to_owned()
            } else if line.contains("/rest/getStarred2?") {
                r#","starred2":{"artist":[{"id":"ar-1","name":"Band","albumCount":1,"starred":"2026-07-18T12:00:00Z"}],"album":[{"id":"al-1","name":"Blue","songCount":1,"duration":120,"starred":"2026-07-18T12:00:00Z"}],"song":[{"id":"tr-1","title":"Blue Sky","size":42,"duration":120,"bitRate":320,"starred":"2026-07-18T12:00:00Z","userRating":5}]}"#.to_owned()
            } else {
                String::new()
            };
            let body = format!(
                "{{\"subsonic-response\":{{\"status\":\"ok\",\"version\":\"1.16.1\"{data}}}}}"
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
            "member".into(),
            "secret".into(),
        )
        .await
        .unwrap();
    let result = client.get_starred().await.unwrap();
    server.await.unwrap();

    assert_eq!(result.artists[0].id, "ar-1");
    assert_eq!(result.albums[0].id, "al-1");
    assert_eq!(result.tracks[0].id, "tr-1");
    assert_eq!(result.tracks[0].user_rating, Some(5));
    let requests = requests.lock().await;
    assert!(requests[2].contains("/rest/getStarred2?"));
}
