use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use yevune_core::{AlbumFilter, AlbumSort, CoreError, MusicClient, SearchPageRequest};

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

async fn spin_search_server(
    search_response: String,
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
        for _ in 0..3 {
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
            let body = if line.contains("/rest/search3?") {
                search_response.clone()
            } else {
                response_for(&line)
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });
    (address, requests, server)
}

fn search_response(artist_count: usize, album_count: usize, track_count: usize) -> String {
    let artists = (0..artist_count)
        .map(|index| {
            format!("{{\"id\":\"ar-{index}\",\"name\":\"Artist {index}\",\"albumCount\":1}}")
        })
        .collect::<Vec<_>>()
        .join(",");
    let albums = (0..album_count)
        .map(|index| {
            format!(
                "{{\"id\":\"al-{index}\",\"name\":\"Album {index}\",\"songCount\":1,\"duration\":120}}"
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let tracks = (0..track_count)
        .map(|index| {
            format!(
                "{{\"id\":\"tr-{index}\",\"title\":\"Track {index}\",\"size\":42,\"duration\":120,\"bitRate\":320}}"
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"subsonic-response\":{{\"status\":\"ok\",\"version\":\"1.16.1\",\"type\":\"yevune-server\",\"serverVersion\":\"0.1.0\",\"openSubsonic\":true,\"searchResult3\":{{\"artist\":[{artists}],\"album\":[{albums}],\"song\":[{tracks}]}}}}}}"
    )
}

#[tokio::test]
async fn search_page_sends_independent_pagination_and_trims_lookahead() {
    let (address, requests, server) = spin_search_server(search_response(3, 2, 0)).await;
    let client = MusicClient::new();
    client
        .login(
            format!("http://{address}"),
            "admin".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap();

    let request = SearchPageRequest {
        query: "blue".into(),
        artist_offset: 3,
        artist_count: 2,
        album_offset: 5,
        album_count: 1,
        track_offset: 7,
        track_count: 0,
    };
    let page = client.search_page(request).await.unwrap();
    server.await.unwrap();

    assert_eq!(page.artists.len(), 2);
    assert!(page.has_more_artists);
    assert_eq!(page.albums.len(), 1);
    assert!(page.has_more_albums);
    assert!(page.tracks.is_empty());
    assert!(!page.has_more_tracks);
    let requests = requests.lock().await;
    let captured = &requests[2];
    assert!(captured.contains("artistOffset=3"));
    assert!(captured.contains("artistCount=3"));
    assert!(captured.contains("albumOffset=5"));
    assert!(captured.contains("albumCount=2"));
    assert!(captured.contains("songOffset=7"));
    assert!(captured.contains("songCount=0"));
}

#[tokio::test]
async fn search_page_count_100_requests_101_items() {
    let (address, requests, server) = spin_search_server(search_response(0, 0, 0)).await;
    let client = MusicClient::new();
    client
        .login(
            format!("http://{address}"),
            "admin".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap();

    client
        .search_page(SearchPageRequest {
            query: "blue".into(),
            artist_offset: 0,
            artist_count: 100,
            album_offset: 0,
            album_count: 100,
            track_offset: 0,
            track_count: 100,
        })
        .await
        .unwrap();
    server.await.unwrap();

    let requests = requests.lock().await;
    assert!(requests[2].contains("artistCount=101"));
    assert!(requests[2].contains("albumCount=101"));
    assert!(requests[2].contains("songCount=101"));
}

#[tokio::test]
async fn search_page_count_101_is_rejected_before_network_request() {
    let (address, requests, server) = spin_server(2).await;
    let client = MusicClient::new();
    client
        .login(
            format!("http://{address}"),
            "admin".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap();

    let result = client
        .search_page(SearchPageRequest {
            query: "blue".into(),
            artist_offset: 0,
            artist_count: 101,
            album_offset: 0,
            album_count: 0,
            track_offset: 0,
            track_count: 0,
        })
        .await;
    server.await.unwrap();

    assert!(matches!(
        result,
        Err(CoreError::InvalidRequest { message }) if message == "search count must be <= 100"
    ));
    assert_eq!(requests.lock().await.len(), 2);
}

#[tokio::test]
async fn search_keeps_legacy_limit_of_20_for_each_result_type() {
    let (address, requests, server) = spin_search_server(search_response(21, 21, 21)).await;
    let client = MusicClient::new();
    client
        .login(
            format!("http://{address}"),
            "admin".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap();

    let result = client.search("blue".into()).await.unwrap();
    server.await.unwrap();

    assert_eq!(result.artists.len(), 20);
    assert_eq!(result.albums.len(), 20);
    assert_eq!(result.tracks.len(), 20);
    let requests = requests.lock().await;
    assert!(requests[2].contains("artistOffset=0"));
    assert!(requests[2].contains("artistCount=21"));
    assert!(requests[2].contains("albumOffset=0"));
    assert!(requests[2].contains("albumCount=21"));
    assert!(requests[2].contains("songOffset=0"));
    assert!(requests[2].contains("songCount=21"));
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
