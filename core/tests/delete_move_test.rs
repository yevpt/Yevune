use yevune_core::MusicClient;

#[tokio::test]
async fn management_methods_require_an_authenticated_session() {
    let client = MusicClient::new();
    assert!(client.delete_track("tr-1".into()).await.is_err());
    assert!(client
        .move_track("tr-1".into(), "library/new.flac".into())
        .await
        .is_err());
}
