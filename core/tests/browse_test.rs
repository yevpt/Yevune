use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use yevune_core::{AlbumFilter, AlbumSort, MusicClient};

#[tokio::test]
async fn browse_and_search_decode_opensubsonic_json_payloads() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let server = tokio::spawn(async move {
        for _ in 0..6 {
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
            let body = response_for(&line);
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
    let albums = client
        .list_albums(AlbumFilter::Sort(AlbumSort::Newest), 0, 50)
        .await
        .unwrap();
    let album = client.get_album("al-1".to_owned()).await.unwrap();
    let artists = client.list_artists().await.unwrap();
    let search = client.search("Blue".to_owned()).await.unwrap();
    server.await.unwrap();

    assert_eq!(albums[0].name, "Blue");
    assert_eq!(album.tracks[0].title, "Blue Sky");
    assert_eq!(artists[0].name, "Band");
    assert_eq!(search.albums[0].id, "al-1");
    let requests = requests.lock().await;
    assert!(requests[2].contains("/rest/getAlbumList2?"));
    assert!(requests[2].contains("type=newest"));
    assert!(requests[3].contains("/rest/getAlbum?"));
    assert!(requests[3].contains("id=al-1"));
    assert!(requests[4].contains("/rest/getArtists?"));
    assert!(requests[5].contains("/rest/search3?"));
    assert!(requests[5].contains("query=Blue"));
}

fn response_for(request: &str) -> String {
    let data = if request.contains("/rest/getUser?") {
        "\"user\":{\"username\":\"admin\",\"adminRole\":true}"
    } else if request.contains("/rest/getAlbumList2") {
        "\"albumList2\":{\"album\":[{\"id\":\"al-1\",\"name\":\"Blue\",\"songCount\":1,\"duration\":120}]}"
    } else if request.contains("/rest/getAlbum?") {
        "\"album\":{\"id\":\"al-1\",\"name\":\"Blue\",\"songCount\":1,\"duration\":120,\"song\":[{\"id\":\"tr-1\",\"title\":\"Blue Sky\",\"size\":42,\"duration\":120,\"bitRate\":320}]}"
    } else if request.contains("/rest/getArtists") {
        "\"artists\":{\"index\":[{\"name\":\"B\",\"artist\":[{\"id\":\"ar-1\",\"name\":\"Band\",\"albumCount\":1}]}]}"
    } else if request.contains("/rest/search3") {
        "\"searchResult3\":{\"artist\":[{\"id\":\"ar-1\",\"name\":\"Band\",\"albumCount\":1}],\"album\":[{\"id\":\"al-1\",\"name\":\"Blue\",\"songCount\":1,\"duration\":120}],\"song\":[{\"id\":\"tr-1\",\"title\":\"Blue Sky\",\"size\":42,\"duration\":120,\"bitRate\":320}]}"
    } else if request.contains("/rest/getGenres") {
        "\"genres\":{\"genre\":[{\"value\":\"Rock\",\"songCount\":5,\"albumCount\":2}]}"
    } else {
        ""
    };
    format!(
        "{{\"subsonic-response\":{{\"status\":\"ok\",\"version\":\"1.16.1\",\"type\":\"yevune-server\",\"serverVersion\":\"0.1.0\",\"openSubsonic\":true{comma}{data}}}}}",
        comma = if data.is_empty() { "" } else { "," }
    )
}

async fn spin_server(
    n: usize,
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
        for _ in 0..n {
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
            let body = response_for(&line);
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
async fn list_albums_by_genre_sends_type_and_genre_query() {
    let (address, requests, server) = spin_server(3).await;
    let client = MusicClient::new();
    client
        .login(
            format!("http://{address}"),
            "admin".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap();
    let albums = client
        .list_albums(AlbumFilter::Genre("Rock".to_owned()), 0, 50)
        .await
        .unwrap();
    server.await.unwrap();

    assert_eq!(albums[0].name, "Blue");
    let requests = requests.lock().await;
    assert!(requests[2].contains("/rest/getAlbumList2?"));
    assert!(requests[2].contains("type=byGenre"));
    assert!(requests[2].contains("genre=Rock"));
}

#[tokio::test]
async fn list_albums_by_year_range_sends_from_and_to_year() {
    let (address, requests, server) = spin_server(3).await;
    let client = MusicClient::new();
    client
        .login(
            format!("http://{address}"),
            "admin".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap();
    let albums = client
        .list_albums(
            AlbumFilter::YearRange {
                from: 2000,
                to: 2020,
            },
            0,
            50,
        )
        .await
        .unwrap();
    server.await.unwrap();

    assert_eq!(albums[0].name, "Blue");
    let requests = requests.lock().await;
    assert!(requests[2].contains("type=byYear"));
    assert!(requests[2].contains("fromYear=2000"));
    assert!(requests[2].contains("toYear=2020"));
}

#[tokio::test]
async fn list_genres_decodes_genre_array() {
    let (address, requests, server) = spin_server(3).await;
    let client = MusicClient::new();
    client
        .login(
            format!("http://{address}"),
            "admin".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap();
    let genres = client.list_genres().await.unwrap();
    server.await.unwrap();

    assert_eq!(genres[0].value, "Rock");
    assert_eq!(genres[0].song_count, 5);
    assert_eq!(genres[0].album_count, 2);
    let requests = requests.lock().await;
    assert!(requests[2].contains("/rest/getGenres"));
}
