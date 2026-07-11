//! 首启引导：无用户时创建管理员（设计文档 §11 / 计划 T10）。

use music_server::auth::Encryptor;
use music_server::index::Index;
use music_server::setup::{ensure_admin, AdminSeed, SetupOutcome};

async fn temp_index() -> (Index, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let index = Index::connect(&dir.path().join("music.sqlite"))
        .await
        .expect("连接并迁移失败");
    (index, dir)
}

#[tokio::test]
async fn 首启无用户时创建管理员() {
    let (index, _dir) = temp_index().await;
    let seed = AdminSeed {
        username: "admin".into(),
        password: Some("s3cret".into()),
    };
    let outcome = ensure_admin(&index, "app-secret", &seed).await.unwrap();
    match outcome {
        SetupOutcome::AdminCreated {
            username,
            generated_password,
        } => {
            assert_eq!(username, "admin");
            assert!(generated_password.is_none(), "已提供密码则不生成");
        }
        SetupOutcome::AlreadyInitialized => panic!("应创建管理员"),
    }
    let users = index.users().list_users().await.unwrap();
    assert_eq!(users.len(), 1);
    let uid: i64 = users[0].id.parse().unwrap();
    assert!(
        index
            .access_control()
            .resolve_viewer(uid)
            .await
            .unwrap()
            .admin
    );
}

#[tokio::test]
async fn 已有用户时幂等跳过() {
    let (index, _dir) = temp_index().await;
    let seed = AdminSeed {
        username: "admin".into(),
        password: Some("s3cret".into()),
    };
    ensure_admin(&index, "app-secret", &seed).await.unwrap();
    // 二次调用不得再建用户。
    let outcome = ensure_admin(&index, "app-secret", &seed).await.unwrap();
    assert!(matches!(outcome, SetupOutcome::AlreadyInitialized));
    assert_eq!(index.users().list_users().await.unwrap().len(), 1);
}

#[tokio::test]
async fn 未提供密码时生成随机密码且可用() {
    let (index, _dir) = temp_index().await;
    let app_secret = "app-secret";
    let seed = AdminSeed {
        username: "admin".into(),
        password: None,
    };
    let outcome = ensure_admin(&index, app_secret, &seed).await.unwrap();
    let SetupOutcome::AdminCreated {
        generated_password: Some(pw),
        ..
    } = outcome
    else {
        panic!("应生成随机密码");
    };
    assert!(pw.len() >= 16, "随机密码需足够长");

    // 生成的密码须与 AppState 一致的加密器可解出，从而可用于认证。
    let encryptor = Encryptor::new(&format!("pwd:{app_secret}"));
    let enc: String = sqlx::query_scalar("SELECT password_enc FROM users WHERE name = 'admin'")
        .fetch_one(index.pool())
        .await
        .unwrap();
    assert_eq!(encryptor.decrypt(&enc).unwrap(), pw);
}
