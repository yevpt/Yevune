use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use yevune_core::MusicClient;

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
    format!("{{\"subsonic-response\":{{\"status\":\"ok\",\"version\":\"1.16.1\",\"type\":\"yevune-server\",\"serverVersion\":\"0.1.0\",\"openSubsonic\":true{}}}}}",
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
async fn playlist_detail_decodes_standard_opensubsonic_shape() {
    // 真实服务端标准 getPlaylist/createPlaylist 响应：owner 是用户名字符串，
    // 无 ownerId/folderId/position（这些仅扩展的 getPlaylistTree 输出）。
    let body = "\"playlist\":{\"id\":\"playlist:7\",\"name\":\"1\",\"owner\":\"admin\",\"public\":false,\"songCount\":0,\"duration\":0,\"entry\":[]}";
    let (address, _requests, handle) = mock_server(vec![ok(""), ok(body)]).await;
    let client = logged_in(address).await;

    let detail = client.playlist_detail("playlist:7".into()).await.unwrap();
    handle.await.unwrap();

    assert_eq!(detail.playlist.id, "playlist:7");
    assert_eq!(detail.playlist.name, "1");
    assert!(detail.tracks.is_empty());
}

#[tokio::test]
async fn create_playlist_decodes_standard_opensubsonic_shape() {
    let created = "\"playlist\":{\"id\":\"playlist:7\",\"name\":\"1\",\"owner\":\"admin\",\"public\":false,\"songCount\":0,\"duration\":0,\"entry\":[]}";
    let (address, _requests, handle) = mock_server(vec![ok(""), ok(created)]).await;
    let client = logged_in(address).await;

    let playlist = client
        .create_playlist("1".into(), None, vec![])
        .await
        .unwrap();
    handle.await.unwrap();

    assert_eq!(playlist.id, "playlist:7");
    assert_eq!(playlist.name, "1");
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

#[tokio::test]
async fn create_playlist_without_folder_sends_single_request() {
    let created = "\"playlist\":{\"id\":\"playlist:7\",\"ownerId\":\"user:1\",\"name\":\"New\",\"comment\":null,\"folderId\":null,\"position\":0,\"songCount\":0,\"duration\":0,\"created\":null,\"changed\":null,\"entry\":[]}";
    let (address, requests, handle) = mock_server(vec![ok(""), ok(created)]).await;
    let client = logged_in(address).await;

    let playlist = client
        .create_playlist("New".into(), None, vec![])
        .await
        .unwrap();
    handle.await.unwrap();

    assert_eq!(playlist.id, "playlist:7");
    let reqs = requests.lock().await;
    assert_eq!(reqs.len(), 2); // ping + createPlaylist，无 move
    assert!(reqs[1].contains("/rest/createPlaylist?"));
    assert!(reqs[1].contains("name=New"));
}

#[tokio::test]
async fn create_playlist_with_folder_creates_then_moves() {
    let created = "\"playlist\":{\"id\":\"playlist:7\",\"ownerId\":\"user:1\",\"name\":\"New\",\"comment\":null,\"folderId\":null,\"position\":0,\"songCount\":0,\"duration\":0,\"created\":null,\"changed\":null,\"entry\":[]}";
    let (address, requests, handle) = mock_server(vec![ok(""), ok(created), ok("")]).await;
    let client = logged_in(address).await;

    let playlist = client
        .create_playlist(
            "New".into(),
            Some("folder:2".into()),
            vec!["track:1".into()],
        )
        .await
        .unwrap();
    handle.await.unwrap();

    assert_eq!(playlist.folder_id.as_deref(), Some("folder:2"));
    let reqs = requests.lock().await;
    assert_eq!(reqs.len(), 3); // ping + create + move
    assert!(reqs[1].contains("songId=track%3A1"));
    assert!(reqs[2].contains("/rest/ext/movePlaylist?"));
    assert!(reqs[2].contains("id=playlist%3A7"));
    assert!(reqs[2].contains("folderId=folder%3A2"));
}

#[tokio::test]
async fn delete_playlist_hits_endpoint() {
    let (address, requests, handle) = mock_server(vec![ok(""), ok("")]).await;
    let client = logged_in(address).await;

    client.delete_playlist("playlist:7".into()).await.unwrap();
    handle.await.unwrap();

    assert!(requests.lock().await[1].contains("/rest/deletePlaylist?"));
}

#[tokio::test]
async fn rename_and_add_and_remove_encode_params() {
    let (address, requests, handle) = mock_server(vec![ok(""), ok(""), ok(""), ok("")]).await;
    let client = logged_in(address).await;

    client
        .rename_playlist("playlist:5".into(), "Renamed".into())
        .await
        .unwrap();
    client
        .add_tracks(
            "playlist:5".into(),
            vec!["track:1".into(), "track:2".into()],
        )
        .await
        .unwrap();
    client
        .remove_track_at("playlist:5".into(), 3)
        .await
        .unwrap();
    handle.await.unwrap();

    let reqs = requests.lock().await;
    assert!(reqs[1].contains("/rest/updatePlaylist?"));
    assert!(reqs[1].contains("playlistId=playlist%3A5"));
    assert!(reqs[1].contains("name=Renamed"));
    assert!(reqs[2].contains("songIdToAdd=track%3A1"));
    assert!(reqs[2].contains("songIdToAdd=track%3A2"));
    assert!(reqs[3].contains("songIndexToRemove=3"));
}

#[tokio::test]
async fn create_folder_decodes_and_move_encodes() {
    let folder = "\"playlistFolder\":{\"id\":\"folder:3\",\"ownerId\":\"user:1\",\"name\":\"Jazz\",\"parentId\":\"folder:1\",\"position\":0}";
    let (address, requests, handle) =
        mock_server(vec![ok(""), ok(folder), ok(""), ok(""), ok("")]).await;
    let client = logged_in(address).await;

    let created = client
        .create_folder("Jazz".into(), Some("folder:1".into()))
        .await
        .unwrap();
    client
        .rename_folder("folder:3".into(), "Bebop".into())
        .await
        .unwrap();
    client.move_folder("folder:3".into(), None).await.unwrap();
    client.delete_folder("folder:3".into()).await.unwrap();
    handle.await.unwrap();

    assert_eq!(created.name, "Jazz");
    assert_eq!(created.parent_id.as_deref(), Some("folder:1"));
    let reqs = requests.lock().await;
    assert!(reqs[1].contains("/rest/ext/createPlaylistFolder?"));
    assert!(reqs[1].contains("parentId=folder%3A1"));
    assert!(reqs[2].contains("/rest/ext/updatePlaylistFolder?"));
    assert!(reqs[3].contains("/rest/ext/moveFolder?"));
    assert!(!reqs[3].contains("parentId=")); // 移到根不带 parentId
    assert!(reqs[4].contains("/rest/ext/deletePlaylistFolder?"));
}
