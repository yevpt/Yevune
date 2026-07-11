use music_core::MusicClient;

#[tokio::test]
async fn scan_operations_require_login() {
    let client = MusicClient::new();
    assert!(client.start_scan().await.is_err());
    assert!(client.scan_status().await.is_err());
}
