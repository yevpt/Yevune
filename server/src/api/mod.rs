//! OpenSubsonic HTTP API 集成层。

mod annotation;
mod browsing;
mod ext;
mod media;
mod playlist;
mod response;
mod scan;
mod search;
mod system;
mod user;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, Weak};

use axum::extract::{FromRef, FromRequestParts, Query};
use axum::http::request::Parts;
use axum::response::Response;
use axum::Router;
use serde::de::DeserializeOwned;
use tokio::sync::{Mutex, OwnedMutexGuard, Semaphore};

use crate::auth::{AuthError, AuthState, CurrentUser};
use crate::index::{Index, Viewer};
use crate::scanner::Scanner;
use crate::storage::ObjectStore;
use crate::transcode::Transcoder;

const DEFAULT_COVER_RESIZE_CONCURRENCY: usize = 2;

/// 全部 API handler 共享的应用状态。
#[derive(Clone)]
pub struct AppState {
    pub(crate) index: Index,
    pub(crate) store: Arc<dyn ObjectStore>,
    pub(crate) scanner: Arc<Scanner>,
    pub(crate) transcoder: Transcoder,
    pub(crate) auth: AuthState,
    pub(crate) default_transcode_format: String,
    pub(crate) default_transcode_bitrate: u32,
    pub(crate) cover_resize_semaphore: Arc<Semaphore>,
    pub(crate) library_operation_locks: Arc<KeyedOperationLocks>,
}

/// 曲库对象变更使用的进程内逐键锁表。
pub(crate) struct KeyedOperationLocks {
    locks: StdMutex<HashMap<String, Weak<Mutex<()>>>>,
}

impl KeyedOperationLocks {
    fn new() -> Self {
        Self {
            locks: StdMutex::new(HashMap::new()),
        }
    }

    /// 按稳定字典序持有全部键，避免多键操作互相死锁。
    pub(crate) async fn lock(&self, keys: impl IntoIterator<Item = String>) -> KeyGuards {
        let mut keys: Vec<_> = keys.into_iter().collect();
        keys.sort_unstable();
        keys.dedup();
        let mutexes = {
            let mut locks = self.locks.lock().unwrap();
            locks.retain(|_, lock| lock.strong_count() > 0);
            keys.into_iter()
                .map(|key| {
                    let entry = locks.entry(key).or_default();
                    entry.upgrade().unwrap_or_else(|| {
                        let mutex = Arc::new(Mutex::new(()));
                        *entry = Arc::downgrade(&mutex);
                        mutex
                    })
                })
                .collect::<Vec<_>>()
        };
        let mut guards = Vec::with_capacity(mutexes.len());
        for mutex in mutexes {
            guards.push(mutex.lock_owned().await);
        }
        KeyGuards { _guards: guards }
    }
}

pub(crate) struct KeyGuards {
    _guards: Vec<OwnedMutexGuard<()>>,
}

impl AppState {
    /// 从已连接的索引、对象存储、应用密钥与 FFmpeg 路径构造 API 状态。
    pub fn new(
        index: Index,
        store: Arc<dyn ObjectStore>,
        app_secret: &str,
        ffmpeg_path: impl Into<PathBuf>,
    ) -> Self {
        Self::with_transcode_defaults(index, store, app_secret, ffmpeg_path, "opus", 128)
    }

    /// 构造 API 状态并注入配置中的默认转码格式与码率。
    pub fn with_transcode_defaults(
        index: Index,
        store: Arc<dyn ObjectStore>,
        app_secret: &str,
        ffmpeg_path: impl Into<PathBuf>,
        default_transcode_format: impl Into<String>,
        default_transcode_bitrate: u32,
    ) -> Self {
        let scanner = Arc::new(Scanner::new(store.clone(), index.clone()));
        let transcoder = Transcoder::new(store.clone(), index.clone(), ffmpeg_path);
        let auth = AuthState::new(index.clone(), app_secret);
        Self {
            index,
            store,
            scanner,
            transcoder,
            auth,
            default_transcode_format: default_transcode_format.into(),
            default_transcode_bitrate,
            cover_resize_semaphore: Arc::new(Semaphore::new(DEFAULT_COVER_RESIZE_CONCURRENCY)),
            library_operation_locks: Arc::new(KeyedOperationLocks::new()),
        }
    }

    /// 解析当前用户为访问控制视角（角色集 + 管理员标记）。
    ///
    /// 供所有曲库读路径在查询前取得 [`Viewer`]，把"查询时评估 + 最具体优先 +
    /// 管理员绕过"的可见性强制在服务端，客户端不可绕过（设计文档 §6）。
    pub(crate) async fn viewer(&self, user_id: i64) -> crate::index::Result<Viewer> {
        self.index.access_control().resolve_viewer(user_id).await
    }
}

impl FromRef<AppState> for AuthState {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}

/// 把 T6 用户提取器的拒绝统一转换为 OpenSubsonic 协议信封。
pub(crate) struct ApiUser(pub CurrentUser);

#[axum::async_trait]
impl FromRequestParts<AppState> for ApiUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let format = response::Format::from_uri(&parts.uri);
        CurrentUser::from_request_parts(parts, state)
            .await
            .map(Self)
            .map_err(|error| response::auth_error(format, error))
    }
}

/// 管理端点提取器，服务端强制 admin 角色。
pub(crate) struct ApiAdmin(pub CurrentUser);

#[axum::async_trait]
impl FromRequestParts<AppState> for ApiAdmin {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let format = response::Format::from_uri(&parts.uri);
        let user = CurrentUser::from_request_parts(parts, state)
            .await
            .map_err(|error| response::auth_error(format, error))?;
        if user.admin {
            Ok(Self(user))
        } else {
            Err(response::auth_error(format, AuthError::Forbidden))
        }
    }
}

/// 查询参数提取器，把 serde 类型错误统一包装为协议错误码 10。
pub(crate) struct ApiQuery<T>(pub T);

#[axum::async_trait]
impl<T> FromRequestParts<AppState> for ApiQuery<T>
where
    T: DeserializeOwned + Send,
{
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let format = response::Format::from_uri(&parts.uri);
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(value)| Self(value))
            .map_err(|_| response::parameter_error(format, "Request parameter is malformed"))
    }
}

/// 构建完整 API 路由树。
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(system::router())
        .merge(browsing::router())
        .merge(search::router())
        .merge(playlist::router())
        .merge(annotation::router())
        .merge(media::router())
        .merge(scan::router())
        .merge(user::router())
        .merge(ext::router())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use std::future::{poll_fn, Future};
    use std::task::Poll;

    use tokio::sync::oneshot;

    use super::*;

    async fn assert_lock_sets_contend(first: Vec<String>, second: Vec<String>) {
        let locks = Arc::new(KeyedOperationLocks::new());
        let first_guard = locks.lock(first).await;
        let (pending_tx, pending_rx) = oneshot::channel();
        let contender_locks = locks.clone();
        let contender = tokio::spawn(async move {
            let mut lock = Box::pin(contender_locks.lock(second));
            let mut pending_tx = Some(pending_tx);
            poll_fn(move |cx| match lock.as_mut().poll(cx) {
                Poll::Pending => {
                    if let Some(tx) = pending_tx.take() {
                        let _ = tx.send(());
                    }
                    Poll::Pending
                }
                Poll::Ready(guard) => Poll::Ready(guard),
            })
            .await
        });

        pending_rx
            .await
            .expect("contender 必须实际 poll 到共享键并进入 Pending");
        drop(first_guard);
        tokio::time::timeout(std::time::Duration::from_secs(1), contender)
            .await
            .expect("释放共享键后 contender 应完成")
            .unwrap();
    }

    #[tokio::test]
    async fn keyed_lock_barrier_covers_shared_destination_and_track() {
        assert_lock_sets_contend(
            vec!["track:1".into(), "object:library/shared.flac".into()],
            vec!["track:2".into(), "object:library/shared.flac".into()],
        )
        .await;
        assert_lock_sets_contend(
            vec!["track:1".into(), "object:library/first.flac".into()],
            vec!["track:1".into(), "object:library/second.flac".into()],
        )
        .await;
    }

    #[tokio::test]
    async fn keyed_lock_table_reclaims_expired_entries_on_next_acquire() {
        let locks = KeyedOperationLocks::new();
        drop(locks.lock(["object:library/old.flac".into()]).await);
        drop(locks.lock(["object:library/new.flac".into()]).await);
        let table = locks.locks.lock().unwrap();
        assert_eq!(table.len(), 1);
        assert!(table.contains_key("object:library/new.flac"));
    }
}
