use std::sync::Arc;

use music_core::MusicClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// 起一个按顺序返回预设响应体的 mock server，并记录每个请求首部行。
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
    format!("{{\"subsonic-response\":{{\"status\":\"ok\",\"version\":\"1.16.1\",\"type\":\"music\",\"serverVersion\":\"0.1.0\",\"openSubsonic\":true{}}}}}",
        if inner.is_empty() { String::new() } else { format!(",{inner}") })
}

async fn logged_in(address: std::net::SocketAddr) -> Arc<MusicClient> {
    // login 先打一次 ping；调用方需在 bodies 首位放一个 ok("") 供 ping 使用。
    let client = MusicClient::new();
    client
        .login(format!("http://{address}"), "admin".into(), "secret".into())
        .await
        .unwrap();
    client
}

#[tokio::test]
async fn playlist_tree_decodes_folders_and_playlists() {
    let tree = "\"playlistTree\":{\"folders\":[{\"id\":\"folder:1\",\"ownerId\":\"user:1\",\"name\":\"Rock\",\"parentId\":null,\"position\":0}],\"playlists\":[{\"id\":\"playlist:5\",\"ownerId\":\"user:1\",\"name\":\"Mix\",\"comment\":null,\"folderId\":\"folder:1\",\"position\":0,\"songCount\":2,\"duration\":300,\"created\":null,\"changed\":null}]}";
    let (address, requests, handle) = mock_server(vec![ok(""), ok(tree)]).await;
    let client = logged_in(address).await;

    let result = client.playlist_tree().await.unwrap();
    handle.await.unwrap();

    assert_eq!(result.folders.len(), 1);
    assert_eq!(result.folders[0].name, "Rock");
    assert_eq!(result.playlists.len(), 1);
    assert_eq!(result.playlists[0].name, "Mix");
    assert_eq!(result.playlists[0].folder_id.as_deref(), Some("folder:1"));
    assert!(requests.lock().await[1].contains("/rest/ext/getPlaylistTree?"));
}

#[tokio::test]
async fn playlist_detail_decodes_playlist_and_entries() {
    let track =
        "{\"id\":\"track:9\",\"title\":\"Song\",\"size\":10,\"duration\":180,\"bitRate\":320}";
    let body = format!(
        "\"playlist\":{{\"id\":\"playlist:5\",\"ownerId\":\"user:1\",\"name\":\"Mix\",\"comment\":null,\"folderId\":null,\"position\":0,\"songCount\":1,\"duration\":180,\"created\":null,\"changed\":null,\"entry\":[{track}]}}"
    );
    let (address, requests, handle) = mock_server(vec![ok(""), ok(&body)]).await;
    let client = logged_in(address).await;

    let detail = client.playlist_detail("playlist:5".into()).await.unwrap();
    handle.await.unwrap();

    assert_eq!(detail.playlist.name, "Mix");
    assert_eq!(detail.tracks.len(), 1);
    assert_eq!(detail.tracks[0].title, "Song");
    let req = requests.lock().await[1].clone();
    assert!(req.contains("/rest/getPlaylist?"));
    assert!(req.contains("id=playlist%3A5"));
}
