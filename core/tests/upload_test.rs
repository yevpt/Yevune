use std::sync::{Arc, Mutex};

use yevune_core::{MusicClient, UploadMetadata, UploadProgress};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn upload_streams_a_local_file_and_reports_progress() {
    let path = std::env::temp_dir().join(format!("yevune-core-upload-{}.flac", std::process::id()));
    std::fs::write(&path, b"small but complete audio fixture").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for request_number in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let request = read_request(&mut socket).await;
            let body = if request_number == 0 {
                "{\"subsonic-response\":{\"status\":\"ok\",\"version\":\"1.16.1\",\"type\":\"yevune-server\",\"serverVersion\":\"0.1.0\",\"openSubsonic\":true}}".to_owned()
            } else {
                assert!(request.starts_with("POST /rest/ext/uploadTrack?"));
                assert!(request.contains("name=\"key\""));
                assert!(request.contains("library/imported.flac"));
                assert!(request.contains("small but complete audio fixture"));
                "{\"subsonic-response\":{\"status\":\"ok\",\"version\":\"1.16.1\",\"type\":\"yevune-server\",\"serverVersion\":\"0.1.0\",\"openSubsonic\":true,\"track\":{\"id\":\"tr-1\",\"title\":\"Imported\",\"size\":32,\"duration\":120,\"bitRate\":320}}}".to_owned()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let values = Arc::new(Mutex::new(Vec::new()));
    let client = MusicClient::new();
    client
        .login(
            format!("http://{address}"),
            "admin".to_owned(),
            "secret".to_owned(),
        )
        .await
        .unwrap();
    let track = client
        .upload_track(
            path.to_string_lossy().into_owned(),
            UploadMetadata {
                library_key: "library/imported.flac".to_owned(),
            },
            Box::new(ProgressProbe {
                values: values.clone(),
            }),
        )
        .await
        .unwrap();
    server.await.unwrap();
    std::fs::remove_file(path).unwrap();

    assert_eq!(track.id, "tr-1");
    assert_eq!(values.lock().unwrap().last(), Some(&(32, 32)));
}

struct ProgressProbe {
    values: Arc<Mutex<Vec<(u64, u64)>>>,
}

impl UploadProgress for ProgressProbe {
    fn on_progress(&self, sent_bytes: u64, total_bytes: u64) {
        self.values.lock().unwrap().push((sent_bytes, total_bytes));
    }
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];
    let header_end = loop {
        let count = socket.read(&mut buffer).await.unwrap();
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then_some(value.trim())
        })
        .map_or(0, |value| value.parse::<usize>().unwrap());
    while bytes.len() < header_end + length {
        let count = socket.read(&mut buffer).await.unwrap();
        bytes.extend_from_slice(&buffer[..count]);
    }
    String::from_utf8(bytes).unwrap()
}
