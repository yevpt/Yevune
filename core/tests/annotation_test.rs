use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use yevune_core::{AnnotationItemType, CoreError, MusicClient};

async fn server_for_requests(
    count: usize,
) -> (
    std::net::SocketAddr,
    Arc<Mutex<Vec<String>>>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let server = tokio::spawn(async move {
        for _ in 0..count {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let bytes = socket.read(&mut request).await.unwrap();
            let line = std::str::from_utf8(&request[..bytes])
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .to_owned();
            observed.lock().await.push(line.clone());
            let payload = if line.contains("/rest/getUser?") {
                ",\"user\":{\"username\":\"member\",\"adminRole\":false}"
            } else {
                ""
            };
            let body = format!(
                "{{\"subsonic-response\":{{\"status\":\"ok\",\"version\":\"1.16.1\",\"openSubsonic\":true{payload}}}}}"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });
    (address, requests, server)
}

#[tokio::test]
async fn annotation_writes_use_standard_endpoints_and_entity_parameters() {
    let (address, requests, server) = server_for_requests(7).await;
    let client = MusicClient::new();
    client
        .login(
            format!("http://{address}"),
            "member".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap();

    client
        .set_starred("tr-1".into(), AnnotationItemType::Track, true)
        .await
        .unwrap();
    client
        .set_starred("al-2".into(), AnnotationItemType::Album, false)
        .await
        .unwrap();
    client
        .set_starred("ar-3".into(), AnnotationItemType::Artist, true)
        .await
        .unwrap();
    client.set_rating("tr-1".into(), Some(5)).await.unwrap();
    client.set_rating("tr-1".into(), None).await.unwrap();
    server.await.unwrap();

    let requests = requests.lock().await;
    assert!(requests[2].contains("/rest/star?"));
    assert!(requests[2].contains("id=tr-1"));
    assert!(requests[3].contains("/rest/unstar?"));
    assert!(requests[3].contains("albumId=al-2"));
    assert!(requests[4].contains("/rest/star?"));
    assert!(requests[4].contains("artistId=ar-3"));
    assert!(requests[5].contains("/rest/setRating?"));
    assert!(requests[5].contains("id=tr-1"));
    assert!(requests[5].contains("rating=5"));
    assert!(requests[6].contains("rating=0"));
}

#[tokio::test]
async fn invalid_rating_is_rejected_before_network_request() {
    let (address, requests, server) = server_for_requests(2).await;
    let client = MusicClient::new();
    client
        .login(
            format!("http://{address}"),
            "member".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap();

    let error = client.set_rating("tr-1".into(), Some(6)).await.unwrap_err();
    assert!(matches!(error, CoreError::InvalidRequest { .. }));
    server.await.unwrap();
    assert_eq!(requests.lock().await.len(), 2);
}
