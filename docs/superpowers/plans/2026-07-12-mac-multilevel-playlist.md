# Mac 多级歌单 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 mac 客户端通过侧栏管理当前用户的多级歌单（文件夹树 + 歌单叶子），完成歌单/文件夹的增删改查、移动与成员增删，全部菜单驱动。

**Architecture:** 服务端歌单能力（标准 OpenSubsonic + `/rest/ext/*` 多级树）已就绪、不改动。本计划先在 `core` crate 补编排层（HTTP 编排、DTO 解码、create+move 组合逻辑），经 UniFFI 暴露；再在 SwiftUI 侧新增 `PlaylistViewModel` 与侧栏/详情界面。Swift 只协调 UI，不解析 HTTP、不复制业务规则。

**Tech Stack:** Rust（`reqwest`、`serde`、`uniffi` 0.31、`tokio`）、Swift（SwiftUI、XCTest）、UniFFI 绑定。

## Global Constraints

- 服务端语言 Rust；不新增后端语言、不新增依赖（本计划零新依赖）。
- 不破坏 OpenSubsonic 兼容性；扩展只走 `/rest/ext/*`。
- 共享 DTO 复用 `contract`，客户端不重复定义数据模型；core 用 `#[uniffi::remote(Record)]` 桥接。
- 授权在服务端强制，客户端只呈现其可见结果。
- TDD：先写失败测试→跑红→最小实现→跑绿→提交；无测试不提交产品代码。
- DoD：`cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check`、Swift `swift test` 全绿。
- 提交遵守 `.agents/skills/git-commit/SKILL.md`：`<type>(<scope>): <中文主题>`，身份 `yevpt <vpt940417@gmail.com>`，禁止 `--author`/`-c user.*`/AI 署名。
- core ext 端点调用用 `"ext/<name>"` 作为 endpoint 名（`http.get_json`/`get_empty_with_params` 会拼成 `/rest/ext/<name>`）。

## 文件结构

- `core/src/ffi_types.rs`（改）：新增 `contract::Playlist`、`contract::PlaylistFolder` 的 `#[uniffi::remote(Record)]`。
- `core/src/api/playlist.rs`（建）：歌单/文件夹的全部 core 函数与 core 自有 `uniffi::Record`（`PlaylistTree`、`PlaylistDetail`）+ 内部 serde 解码结构。
- `core/src/api/mod.rs`（改）：`pub(crate) mod playlist;`。
- `core/src/client.rs`（改）：`MusicClient` 门面转发歌单方法。
- `core/tests/playlist_test.rs`（建）：mock server 集成测试。
- `clients/apple/Sources/MusicApp/Model/LoginViewModel.swift`（改）：`MusicClientProviding` 增歌单方法 + 默认实现扩展。
- `clients/apple/Sources/MusicApp/Model/CoreMusicClient.swift`（改）：桥接歌单方法到 CoreFFI。
- `clients/apple/Sources/MusicApp/Model/PlaylistViewModel.swift`（建）：树/详情状态 + CRUD。
- `clients/apple/Sources/MusicApp/Views/PlaylistDetailView.swift`（建）：歌单曲目详情。
- `clients/apple/Sources/MusicApp/Views/LibraryView.swift`（改）：侧栏分区 + `SidebarSelection` + 菜单接线。
- `clients/apple/Sources/MusicApp/Views/MediaDetailView.swift`（改）：曲目行「加入歌单」子菜单。
- `clients/apple/Tests/MusicAppTests/PlaylistViewModelTests.swift`（建）：ViewModel XCTest + `FakePlaylistClient`。

---

### Task 1: core — DTO 桥接 + 读取歌单树

**Files:**
- Modify: `core/src/ffi_types.rs`
- Modify: `core/src/api/mod.rs`
- Create: `core/src/api/playlist.rs`
- Modify: `core/src/client.rs`
- Create: `core/tests/playlist_test.rs`

**Interfaces:**
- Consumes: `HttpClient::get_json`、`AuthenticatedSession`、`contract::{Playlist, PlaylistFolder}`。
- Produces:
  - `core::PlaylistTree { folders: Vec<PlaylistFolder>, playlists: Vec<Playlist> }`（uniffi Record）
  - `playlist::playlist_tree(http, auth) -> Result<PlaylistTree>`
  - `MusicClient::playlist_tree() -> Result<PlaylistTree>`

- [ ] **Step 1: 写失败测试**

在 `core/tests/playlist_test.rs`：

```rust
use std::sync::Arc;

use music_core::MusicClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// 起一个按顺序返回预设响应体的 mock server，并记录每个请求首部行。
async fn mock_server(bodies: Vec<String>) -> (std::net::SocketAddr, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let handle = tokio::spawn(async move {
        for body in bodies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let count = socket.read(&mut bytes).await.unwrap();
            observed.lock().await.push(String::from_utf8_lossy(&bytes[..count]).into_owned());
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

async fn logged_in(address: std::net::SocketAddr) -> MusicClient {
    // login 先打一次 ping；调用方需在 bodies 首位放一个 ok("") 供 ping 使用。
    let client = MusicClient::new();
    client.login(format!("http://{address}"), "admin".into(), "secret".into()).await.unwrap();
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p music-core --test playlist_test playlist_tree_decodes_folders_and_playlists`
Expected: 编译失败（`playlist_tree` 未定义）。

- [ ] **Step 3: 桥接 DTO**

在 `core/src/ffi_types.rs` 顶部 `use` 增加 `PlaylistFolder, Playlist`，并追加：

```rust
#[uniffi::remote(Record)]
pub struct PlaylistFolder {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub position: u32,
}

#[uniffi::remote(Record)]
pub struct Playlist {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub comment: Option<String>,
    pub folder_id: Option<String>,
    pub position: u32,
    pub song_count: u32,
    pub duration: u32,
    pub created: Option<String>,
    pub changed: Option<String>,
}
```

即把 `use contract::{Album, Artist, Track};` 改为 `use contract::{Album, Artist, Playlist, PlaylistFolder, Track};`。

- [ ] **Step 4: 建 playlist.rs 并实现 tree 读取**

在 `core/src/api/mod.rs` 的模块列表按字母序插入 `pub(crate) mod playlist;`。

新建 `core/src/api/playlist.rs`：

```rust
//! 当前用户多级歌单：树读取、详情、增删改查与组织。

use contract::{Playlist, PlaylistFolder, Track};
use serde::Deserialize;

use crate::auth::AuthenticatedSession;
use crate::error::Result;
use crate::http::HttpClient;

/// 一次性拿到的歌单文件夹树与叶子歌单，层级由 UI 本地组装。
#[derive(Clone, uniffi::Record)]
pub struct PlaylistTree {
    pub folders: Vec<PlaylistFolder>,
    pub playlists: Vec<Playlist>,
}

pub(crate) async fn playlist_tree(
    http: &HttpClient,
    auth: &AuthenticatedSession,
) -> Result<PlaylistTree> {
    let payload: TreePayload = http.get_json(auth, "ext/getPlaylistTree", &[]).await?;
    Ok(PlaylistTree {
        folders: payload.playlist_tree.folders,
        playlists: payload.playlist_tree.playlists,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TreePayload {
    playlist_tree: TreeBody,
}

#[derive(Deserialize)]
struct TreeBody {
    #[serde(default)]
    folders: Vec<PlaylistFolder>,
    #[serde(default)]
    playlists: Vec<Playlist>,
}
```

在 `core/src/client.rs`：`use` 段增加 `use crate::api::playlist::{self, PlaylistTree};`，并在 `#[uniffi::export]` impl 内追加：

```rust
    /// 读取当前用户的歌单文件夹树与叶子歌单。
    pub async fn playlist_tree(&self) -> Result<PlaylistTree> {
        playlist::playlist_tree(&self.http, &self.authenticated_session().await?).await
    }
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p music-core --test playlist_test`
Expected: PASS。

- [ ] **Step 6: 提交**

```bash
cargo fmt && cargo clippy -p music-core -- -D warnings
git add core/src/ffi_types.rs core/src/api/mod.rs core/src/api/playlist.rs core/src/client.rs core/tests/playlist_test.rs
git commit -m "feat(core): 读取多级歌单树"
```

---

### Task 2: core — 读取歌单详情

**Files:**
- Modify: `core/src/api/playlist.rs`
- Modify: `core/src/client.rs`
- Modify: `core/tests/playlist_test.rs`

**Interfaces:**
- Consumes: Task 1 的 helper（`mock_server`、`ok`、`logged_in`）。
- Produces:
  - `core::PlaylistDetail { playlist: Playlist, tracks: Vec<Track> }`
  - `playlist::playlist_detail(http, auth, id) -> Result<PlaylistDetail>`
  - `MusicClient::playlist_detail(id) -> Result<PlaylistDetail>`

- [ ] **Step 1: 写失败测试**

追加到 `core/tests/playlist_test.rs`：

```rust
#[tokio::test]
async fn playlist_detail_decodes_playlist_and_entries() {
    let track = "{\"id\":\"track:9\",\"title\":\"Song\",\"size\":10,\"duration\":180,\"bitRate\":320}";
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p music-core --test playlist_test playlist_detail_decodes_playlist_and_entries`
Expected: 编译失败（`playlist_detail` 未定义）。

- [ ] **Step 3: 实现**

在 `core/src/api/playlist.rs` 增加：

```rust
/// 歌单及其（经服务端访问控制过滤后的）曲目。
#[derive(Clone, uniffi::Record)]
pub struct PlaylistDetail {
    pub playlist: Playlist,
    pub tracks: Vec<Track>,
}

pub(crate) async fn playlist_detail(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<PlaylistDetail> {
    let payload: DetailPayload = http
        .get_json(auth, "getPlaylist", &[("id".to_owned(), id)])
        .await?;
    Ok(PlaylistDetail {
        playlist: payload.playlist.playlist,
        tracks: payload.playlist.entry,
    })
}
```

以及内部结构（放到文件末尾的解码结构区）：

```rust
#[derive(Deserialize)]
struct DetailPayload {
    playlist: PlaylistWithEntries,
}

#[derive(Deserialize)]
struct PlaylistWithEntries {
    #[serde(flatten)]
    playlist: Playlist,
    #[serde(default)]
    entry: Vec<Track>,
}
```

在 `core/src/client.rs` 的 `use` 改为 `use crate::api::playlist::{self, PlaylistDetail, PlaylistTree};`，并追加门面：

```rust
    /// 读取单个歌单及其曲目。
    pub async fn playlist_detail(&self, id: String) -> Result<PlaylistDetail> {
        playlist::playlist_detail(&self.http, &self.authenticated_session().await?, id).await
    }
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p music-core --test playlist_test`
Expected: PASS（两个测试）。

- [ ] **Step 5: 提交**

```bash
cargo fmt && cargo clippy -p music-core -- -D warnings
git add core/src/api/playlist.rs core/src/client.rs core/tests/playlist_test.rs
git commit -m "feat(core): 读取歌单详情"
```

---

### Task 3: core — 创建（含落入文件夹）、移动、删除歌单

**Files:**
- Modify: `core/src/api/playlist.rs`
- Modify: `core/src/client.rs`
- Modify: `core/tests/playlist_test.rs`

**Interfaces:**
- Consumes: Task 2 的 `DetailPayload`（复用解码创建返回的歌单）。
- Produces:
  - `playlist::create_playlist(http, auth, name, folder_id: Option<String>, song_ids: Vec<String>) -> Result<Playlist>`
  - `playlist::move_playlist(http, auth, id, folder_id: Option<String>) -> Result<()>`
  - `playlist::delete_playlist(http, auth, id) -> Result<()>`
  - 对应 `MusicClient` 门面同名方法。

**说明（组合逻辑）：** `createPlaylist` 端点不接受 folderId。`create_playlist` 先建歌单，若 `folder_id` 非空再调 `move_playlist` 落位，并把返回歌单的 `folder_id` 本地置为该值后返回（position 保持服务端初值；UI 创建后会整树刷新）。

- [ ] **Step 1: 写失败测试**

追加：

```rust
#[tokio::test]
async fn create_playlist_without_folder_sends_single_request() {
    let created = "\"playlist\":{\"id\":\"playlist:7\",\"ownerId\":\"user:1\",\"name\":\"New\",\"comment\":null,\"folderId\":null,\"position\":0,\"songCount\":0,\"duration\":0,\"created\":null,\"changed\":null,\"entry\":[]}";
    let (address, requests, handle) = mock_server(vec![ok(""), ok(created)]).await;
    let client = logged_in(address).await;

    let playlist = client.create_playlist("New".into(), None, vec![]).await.unwrap();
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
        .create_playlist("New".into(), Some("folder:2".into()), vec!["track:1".into()])
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p music-core --test playlist_test create_playlist_without_folder_sends_single_request`
Expected: 编译失败（方法未定义）。

- [ ] **Step 3: 实现**

在 `core/src/api/playlist.rs` 增加：

```rust
pub(crate) async fn create_playlist(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    name: String,
    folder_id: Option<String>,
    song_ids: Vec<String>,
) -> Result<Playlist> {
    let mut params = vec![("name".to_owned(), name)];
    for song in song_ids {
        params.push(("songId".to_owned(), song));
    }
    let payload: DetailPayload = http.get_json(auth, "createPlaylist", &params).await?;
    let mut playlist = payload.playlist.playlist;
    if let Some(folder_id) = folder_id {
        move_playlist(http, auth, playlist.id.clone(), Some(folder_id.clone())).await?;
        playlist.folder_id = Some(folder_id);
    }
    Ok(playlist)
}

pub(crate) async fn move_playlist(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    folder_id: Option<String>,
) -> Result<()> {
    let mut params = vec![("id".to_owned(), id)];
    if let Some(folder_id) = folder_id {
        params.push(("folderId".to_owned(), folder_id));
    }
    http.get_empty_with_params(auth, "ext/movePlaylist", &params).await
}

pub(crate) async fn delete_playlist(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<()> {
    http.get_empty_with_params(auth, "deletePlaylist", &[("id".to_owned(), id)])
        .await
}
```

在 `core/src/client.rs` 门面追加：

```rust
    /// 创建歌单；`folder_id` 非空时创建后移动到该文件夹。
    pub async fn create_playlist(
        &self,
        name: String,
        folder_id: Option<String>,
        song_ids: Vec<String>,
    ) -> Result<Playlist> {
        playlist::create_playlist(
            &self.http,
            &self.authenticated_session().await?,
            name,
            folder_id,
            song_ids,
        )
        .await
    }

    /// 把歌单移动到指定文件夹；`folder_id` 为 `None` 表示移到根。
    pub async fn move_playlist(&self, id: String, folder_id: Option<String>) -> Result<()> {
        playlist::move_playlist(&self.http, &self.authenticated_session().await?, id, folder_id)
            .await
    }

    /// 删除歌单。
    pub async fn delete_playlist(&self, id: String) -> Result<()> {
        playlist::delete_playlist(&self.http, &self.authenticated_session().await?, id).await
    }
```

`client.rs` 顶部 `use` 段确保引入 `contract::Playlist`（`playlist_tree` 返回里已间接用到，但此处直接返回 `Playlist`，需 `use contract::Playlist;` 或以 `contract::Playlist` 全路径书写）。用全路径 `contract::Playlist` 即可，无需额外 use。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p music-core --test playlist_test`
Expected: PASS（5 个测试）。

- [ ] **Step 5: 提交**

```bash
cargo fmt && cargo clippy -p music-core -- -D warnings
git add core/src/api/playlist.rs core/src/client.rs core/tests/playlist_test.rs
git commit -m "feat(core): 创建移动与删除歌单"
```

---

### Task 4: core — 歌单编辑：改名、备注、增曲、移曲

**Files:**
- Modify: `core/src/api/playlist.rs`
- Modify: `core/src/client.rs`
- Modify: `core/tests/playlist_test.rs`

**Interfaces:**
- Produces（均 `-> Result<()>`）：
  - `rename_playlist(http, auth, id, name)`
  - `set_playlist_comment(http, auth, id, comment)`
  - `add_tracks(http, auth, id, song_ids: Vec<String>)`
  - `remove_track_at(http, auth, id, index: i64)`
  - 对应 `MusicClient` 门面同名方法。

- [ ] **Step 1: 写失败测试**

追加：

```rust
#[tokio::test]
async fn rename_and_add_and_remove_encode_params() {
    let (address, requests, handle) = mock_server(vec![ok(""), ok(""), ok(""), ok("")]).await;
    let client = logged_in(address).await;

    client.rename_playlist("playlist:5".into(), "Renamed".into()).await.unwrap();
    client.add_tracks("playlist:5".into(), vec!["track:1".into(), "track:2".into()]).await.unwrap();
    client.remove_track_at("playlist:5".into(), 3).await.unwrap();
    handle.await.unwrap();

    let reqs = requests.lock().await;
    assert!(reqs[1].contains("/rest/updatePlaylist?"));
    assert!(reqs[1].contains("playlistId=playlist%3A5"));
    assert!(reqs[1].contains("name=Renamed"));
    assert!(reqs[2].contains("songIdToAdd=track%3A1"));
    assert!(reqs[2].contains("songIdToAdd=track%3A2"));
    assert!(reqs[3].contains("songIndexToRemove=3"));
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p music-core --test playlist_test rename_and_add_and_remove_encode_params`
Expected: 编译失败。

- [ ] **Step 3: 实现**

在 `core/src/api/playlist.rs` 增加：

```rust
pub(crate) async fn rename_playlist(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    name: String,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "updatePlaylist",
        &[("playlistId".to_owned(), id), ("name".to_owned(), name)],
    )
    .await
}

pub(crate) async fn set_playlist_comment(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    comment: String,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "updatePlaylist",
        &[("playlistId".to_owned(), id), ("comment".to_owned(), comment)],
    )
    .await
}

pub(crate) async fn add_tracks(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    song_ids: Vec<String>,
) -> Result<()> {
    let mut params = vec![("playlistId".to_owned(), id)];
    for song in song_ids {
        params.push(("songIdToAdd".to_owned(), song));
    }
    http.get_empty_with_params(auth, "updatePlaylist", &params).await
}

pub(crate) async fn remove_track_at(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    index: i64,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "updatePlaylist",
        &[
            ("playlistId".to_owned(), id),
            ("songIndexToRemove".to_owned(), index.to_string()),
        ],
    )
    .await
}
```

在 `core/src/client.rs` 门面追加四个转发方法（签名与上面一致，去掉 `http/auth`，内部用 `&self.http, &self.authenticated_session().await?`）：

```rust
    /// 重命名歌单。
    pub async fn rename_playlist(&self, id: String, name: String) -> Result<()> {
        playlist::rename_playlist(&self.http, &self.authenticated_session().await?, id, name).await
    }

    /// 设置歌单备注。
    pub async fn set_playlist_comment(&self, id: String, comment: String) -> Result<()> {
        playlist::set_playlist_comment(&self.http, &self.authenticated_session().await?, id, comment)
            .await
    }

    /// 向歌单追加曲目。
    pub async fn add_tracks(&self, id: String, song_ids: Vec<String>) -> Result<()> {
        playlist::add_tracks(&self.http, &self.authenticated_session().await?, id, song_ids).await
    }

    /// 按索引移除歌单中的一条曲目。
    pub async fn remove_track_at(&self, id: String, index: i64) -> Result<()> {
        playlist::remove_track_at(&self.http, &self.authenticated_session().await?, id, index).await
    }
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p music-core --test playlist_test`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
cargo fmt && cargo clippy -p music-core -- -D warnings
git add core/src/api/playlist.rs core/src/client.rs core/tests/playlist_test.rs
git commit -m "feat(core): 编辑歌单名备注与成员"
```

---

### Task 5: core — 文件夹增删改与移动

**Files:**
- Modify: `core/src/api/playlist.rs`
- Modify: `core/src/client.rs`
- Modify: `core/tests/playlist_test.rs`

**Interfaces:**
- Produces:
  - `create_folder(http, auth, name, parent_id: Option<String>) -> Result<PlaylistFolder>`
  - `rename_folder(http, auth, id, name) -> Result<()>`
  - `delete_folder(http, auth, id) -> Result<()>`
  - `move_folder(http, auth, id, parent_id: Option<String>) -> Result<()>`
  - 对应 `MusicClient` 门面同名方法。

- [ ] **Step 1: 写失败测试**

追加：

```rust
#[tokio::test]
async fn create_folder_decodes_and_move_encodes() {
    let folder = "\"playlistFolder\":{\"id\":\"folder:3\",\"ownerId\":\"user:1\",\"name\":\"Jazz\",\"parentId\":\"folder:1\",\"position\":0}";
    let (address, requests, handle) = mock_server(vec![ok(""), ok(folder), ok(""), ok(""), ok("")]).await;
    let client = logged_in(address).await;

    let created = client.create_folder("Jazz".into(), Some("folder:1".into())).await.unwrap();
    client.rename_folder("folder:3".into(), "Bebop".into()).await.unwrap();
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p music-core --test playlist_test create_folder_decodes_and_move_encodes`
Expected: 编译失败。

- [ ] **Step 3: 实现**

在 `core/src/api/playlist.rs` 增加：

```rust
pub(crate) async fn create_folder(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    name: String,
    parent_id: Option<String>,
) -> Result<PlaylistFolder> {
    let mut params = vec![("name".to_owned(), name)];
    if let Some(parent_id) = parent_id {
        params.push(("parentId".to_owned(), parent_id));
    }
    let payload: FolderPayload = http.get_json(auth, "ext/createPlaylistFolder", &params).await?;
    Ok(payload.playlist_folder)
}

pub(crate) async fn rename_folder(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    name: String,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "ext/updatePlaylistFolder",
        &[("id".to_owned(), id), ("name".to_owned(), name)],
    )
    .await
}

pub(crate) async fn delete_folder(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<()> {
    http.get_empty_with_params(auth, "ext/deletePlaylistFolder", &[("id".to_owned(), id)])
        .await
}

pub(crate) async fn move_folder(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    parent_id: Option<String>,
) -> Result<()> {
    let mut params = vec![("id".to_owned(), id)];
    if let Some(parent_id) = parent_id {
        params.push(("parentId".to_owned(), parent_id));
    }
    http.get_empty_with_params(auth, "ext/moveFolder", &params).await
}
```

内部解码结构（放解码区）：

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderPayload {
    playlist_folder: PlaylistFolder,
}
```

在 `core/src/client.rs` 门面追加四个转发方法：

```rust
    /// 创建歌单文件夹；`parent_id` 非空时挂到该父文件夹下。
    pub async fn create_folder(
        &self,
        name: String,
        parent_id: Option<String>,
    ) -> Result<contract::PlaylistFolder> {
        playlist::create_folder(&self.http, &self.authenticated_session().await?, name, parent_id)
            .await
    }

    /// 重命名歌单文件夹。
    pub async fn rename_folder(&self, id: String, name: String) -> Result<()> {
        playlist::rename_folder(&self.http, &self.authenticated_session().await?, id, name).await
    }

    /// 删除歌单文件夹（服务端会一并移除其内歌单）。
    pub async fn delete_folder(&self, id: String) -> Result<()> {
        playlist::delete_folder(&self.http, &self.authenticated_session().await?, id).await
    }

    /// 把文件夹移动到新父文件夹；`parent_id` 为 `None` 表示移到根。服务端拒绝成环。
    pub async fn move_folder(&self, id: String, parent_id: Option<String>) -> Result<()> {
        playlist::move_folder(&self.http, &self.authenticated_session().await?, id, parent_id).await
    }
```

- [ ] **Step 4: 跑测试确认通过 + 全量 core 检查**

Run: `cargo test -p music-core && cargo clippy -p music-core -- -D warnings && cargo fmt --check`
Expected: 全 PASS。

- [ ] **Step 5: 提交**

```bash
git add core/src/api/playlist.rs core/src/client.rs core/tests/playlist_test.rs
git commit -m "feat(core): 管理歌单文件夹"
```

---

### Task 6: 重建 UniFFI 绑定 + Swift 桥接协议

**Files:**
- Modify: `clients/apple/Sources/MusicApp/Model/LoginViewModel.swift`
- Modify: `clients/apple/Sources/MusicApp/Model/CoreMusicClient.swift`

**Interfaces:**
- Consumes: CoreFFI 生成类型 `PlaylistTree`、`PlaylistDetail`、`Playlist`、`PlaylistFolder`（来自 Task 1–5）。
- Produces: `MusicClientProviding` 新增歌单方法（带 throwing 默认实现，供既有 Fake 免改）；`CoreMusicClient` 实现转发。

- [ ] **Step 1: 重建 CoreFFI 绑定**

按仓库既有方式重建 UniFFI 绑定与 xcframework（与 `scripts/run-mac-client.sh` 使用的构建链一致）。先确认脚本内的绑定生成命令：

Run: `sed -n '1,60p' scripts/run-mac-client.sh`
然后执行其中的 core 构建 / uniffi-bindgen 段，使 `clients/apple/Packages/CoreFFI/Sources/CoreFFI/CoreFFI.swift` 含 `playlistTree`、`playlistDetail`、`createPlaylist` 等新方法。

- [ ] **Step 2: 校验绑定含新方法**

Run: `grep -n "func playlistTree\|func playlistDetail\|func createPlaylist\|func createFolder" clients/apple/Packages/CoreFFI/Sources/CoreFFI/CoreFFI.swift`
Expected: 命中全部四项。若无命中，回到 Step 1 检查构建命令。

- [ ] **Step 3: 扩协议 + 默认实现**

在 `LoginViewModel.swift` 的 `protocol MusicClientProviding` 尾部（`streamURL` 之后、`}` 之前）追加：

```swift
    func playlistTree() async throws -> PlaylistTree
    func playlistDetail(id: String) async throws -> PlaylistDetail
    func createPlaylist(name: String, folderID: String?, songIDs: [String]) async throws -> Playlist
    func renamePlaylist(id: String, name: String) async throws
    func setPlaylistComment(id: String, comment: String) async throws
    func addTracks(id: String, songIDs: [String]) async throws
    func removeTrackAt(id: String, index: Int64) async throws
    func deletePlaylist(id: String) async throws
    func movePlaylist(id: String, folderID: String?) async throws
    func createFolder(name: String, parentID: String?) async throws -> PlaylistFolder
    func renameFolder(id: String, name: String) async throws
    func deleteFolder(id: String) async throws
    func moveFolder(id: String, parentID: String?) async throws
```

在其后的 `extension MusicClientProviding` 内追加默认实现（沿用既有 `CocoaError(.featureUnsupported)` 风格），使旧的 `FakeMusicClient` 无需改动即可编译：

```swift
    func playlistTree() async throws -> PlaylistTree { throw CocoaError(.featureUnsupported) }
    func playlistDetail(id: String) async throws -> PlaylistDetail { throw CocoaError(.featureUnsupported) }
    func createPlaylist(name: String, folderID: String?, songIDs: [String]) async throws -> Playlist { throw CocoaError(.featureUnsupported) }
    func renamePlaylist(id: String, name: String) async throws { throw CocoaError(.featureUnsupported) }
    func setPlaylistComment(id: String, comment: String) async throws { throw CocoaError(.featureUnsupported) }
    func addTracks(id: String, songIDs: [String]) async throws { throw CocoaError(.featureUnsupported) }
    func removeTrackAt(id: String, index: Int64) async throws { throw CocoaError(.featureUnsupported) }
    func deletePlaylist(id: String) async throws { throw CocoaError(.featureUnsupported) }
    func movePlaylist(id: String, folderID: String?) async throws { throw CocoaError(.featureUnsupported) }
    func createFolder(name: String, parentID: String?) async throws -> PlaylistFolder { throw CocoaError(.featureUnsupported) }
    func renameFolder(id: String, name: String) async throws { throw CocoaError(.featureUnsupported) }
    func deleteFolder(id: String) async throws { throw CocoaError(.featureUnsupported) }
    func moveFolder(id: String, parentID: String?) async throws { throw CocoaError(.featureUnsupported) }
```

- [ ] **Step 4: 实现 CoreMusicClient 转发**

在 `CoreMusicClient.swift` 的 actor 内追加（注意 UniFFI 生成的参数名为 `folderId`/`songIds`/`parentId`/`index`）：

```swift
    func playlistTree() async throws -> PlaylistTree { try await client.playlistTree() }
    func playlistDetail(id: String) async throws -> PlaylistDetail { try await client.playlistDetail(id: id) }
    func createPlaylist(name: String, folderID: String?, songIDs: [String]) async throws -> Playlist {
        try await client.createPlaylist(name: name, folderId: folderID, songIds: songIDs)
    }
    func renamePlaylist(id: String, name: String) async throws { try await client.renamePlaylist(id: id, name: name) }
    func setPlaylistComment(id: String, comment: String) async throws { try await client.setPlaylistComment(id: id, comment: comment) }
    func addTracks(id: String, songIDs: [String]) async throws { try await client.addTracks(id: id, songIds: songIDs) }
    func removeTrackAt(id: String, index: Int64) async throws { try await client.removeTrackAt(id: id, index: index) }
    func deletePlaylist(id: String) async throws { try await client.deletePlaylist(id: id) }
    func movePlaylist(id: String, folderID: String?) async throws { try await client.movePlaylist(id: id, folderId: folderID) }
    func createFolder(name: String, parentID: String?) async throws -> PlaylistFolder {
        try await client.createFolder(name: name, parentId: parentID)
    }
    func renameFolder(id: String, name: String) async throws { try await client.renameFolder(id: id, name: name) }
    func deleteFolder(id: String) async throws { try await client.deleteFolder(id: id) }
    func moveFolder(id: String, parentID: String?) async throws { try await client.moveFolder(id: id, parentId: parentID) }
```

- [ ] **Step 5: 编译校验**

Run: `cd clients/apple && swift build`
Expected: 编译通过（既有测试与代码不受影响）。

- [ ] **Step 6: 提交**

```bash
git add clients/apple/Packages/CoreFFI clients/apple/Sources/MusicApp/Model/LoginViewModel.swift clients/apple/Sources/MusicApp/Model/CoreMusicClient.swift
git commit -m "feat(mac): 桥接歌单 core 接口"
```

---

### Task 7: PlaylistViewModel — 树/详情加载与刷新

**Files:**
- Create: `clients/apple/Sources/MusicApp/Model/PlaylistViewModel.swift`
- Create: `clients/apple/Tests/MusicAppTests/PlaylistViewModelTests.swift`

**Interfaces:**
- Consumes: `MusicClientProviding` 歌单方法（Task 6）。
- Produces: `@MainActor final class PlaylistViewModel: ObservableObject`，`@Published private(set) var tree: PlaylistTree?`、`var detail: PlaylistDetail?`、`var errorMessage: String?`；方法 `loadTree()`、`openPlaylist(id:)`。

- [ ] **Step 1: 写失败测试**

`clients/apple/Tests/MusicAppTests/PlaylistViewModelTests.swift`：

```swift
import XCTest
import CoreFFI
@testable import MusicApp

@MainActor
final class PlaylistViewModelTests: XCTestCase {
    func testLoadTreePublishesFoldersAndPlaylists() async {
        let tree = PlaylistTree(
            folders: [PlaylistFolder(id: "folder:1", ownerId: "user:1", name: "Rock", parentId: nil, position: 0)],
            playlists: [playlistFixture(id: "playlist:5", name: "Mix", folderID: "folder:1")]
        )
        let model = PlaylistViewModel(client: FakePlaylistClient(tree: tree))

        await model.loadTree()

        XCTAssertEqual(model.tree?.folders.first?.name, "Rock")
        XCTAssertEqual(model.tree?.playlists.first?.name, "Mix")
        XCTAssertNil(model.errorMessage)
    }

    func testOpenPlaylistLoadsDetail() async {
        let detail = PlaylistDetail(
            playlist: playlistFixture(id: "playlist:5", name: "Mix", folderID: nil),
            tracks: [trackFixture(id: "track:9", title: "Song")]
        )
        let model = PlaylistViewModel(client: FakePlaylistClient(detail: detail))

        await model.openPlaylist(id: "playlist:5")

        XCTAssertEqual(model.detail?.tracks.first?.title, "Song")
    }
}
```

并在同文件底部提供 fixtures 与 Fake（后续 Task 8 复用同一 Fake）：

```swift
@MainActor
func playlistFixture(id: String, name: String, folderID: String?) -> Playlist {
    Playlist(id: id, ownerId: "user:1", name: name, comment: nil, folderId: folderID,
             position: 0, songCount: 0, duration: 0, created: nil, changed: nil)
}

func trackFixture(id: String, title: String) -> Track {
    Track(id: id, title: title, album: nil, albumId: nil, artist: nil, artistId: nil,
          track: nil, discNumber: nil, year: nil, genre: nil, coverArt: nil, size: 0,
          contentType: nil, suffix: nil, duration: 0, bitRate: 0, created: nil)
}

/// 记录调用并返回预设值的歌单假客户端。其余协议方法走默认 featureUnsupported 实现。
final class FakePlaylistClient: MusicClientProviding, @unchecked Sendable {
    var tree: PlaylistTree
    var detail: PlaylistDetail?
    private(set) var calls: [String] = []

    init(tree: PlaylistTree = PlaylistTree(folders: [], playlists: []), detail: PlaylistDetail? = nil) {
        self.tree = tree
        self.detail = detail
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user)
    }
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }
    func search(query: String) async throws -> SearchResult { SearchResult(artists: [], albums: [], tracks: []) }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw CocoaError(.featureUnsupported)
    }
    func updateTags(id: String, update: TagUpdate) async throws {}
    func deleteTrack(id: String) async throws {}
    func moveTrack(id: String, key: String) async throws {}
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }

    func playlistTree() async throws -> PlaylistTree { calls.append("tree"); return tree }
    func playlistDetail(id: String) async throws -> PlaylistDetail {
        calls.append("detail:\(id)")
        return detail ?? PlaylistDetail(playlist: await playlistFixture(id: id, name: "?", folderID: nil), tracks: [])
    }
    func createPlaylist(name: String, folderID: String?, songIDs: [String]) async throws -> Playlist {
        calls.append("create:\(name):\(folderID ?? "-")")
        return await playlistFixture(id: "playlist:new", name: name, folderID: folderID)
    }
    func renamePlaylist(id: String, name: String) async throws { calls.append("rename:\(id):\(name)") }
    func setPlaylistComment(id: String, comment: String) async throws { calls.append("comment:\(id)") }
    func addTracks(id: String, songIDs: [String]) async throws { calls.append("add:\(id):\(songIDs.joined(separator: ","))") }
    func removeTrackAt(id: String, index: Int64) async throws { calls.append("remove:\(id):\(index)") }
    func deletePlaylist(id: String) async throws { calls.append("delete:\(id)") }
    func movePlaylist(id: String, folderID: String?) async throws { calls.append("move:\(id):\(folderID ?? "-")") }
    func createFolder(name: String, parentID: String?) async throws -> PlaylistFolder {
        calls.append("createFolder:\(name):\(parentID ?? "-")")
        return PlaylistFolder(id: "folder:new", ownerId: "user:1", name: name, parentId: parentID, position: 0)
    }
    func renameFolder(id: String, name: String) async throws { calls.append("renameFolder:\(id):\(name)") }
    func deleteFolder(id: String) async throws { calls.append("deleteFolder:\(id)") }
    func moveFolder(id: String, parentID: String?) async throws { calls.append("moveFolder:\(id):\(parentID ?? "-")") }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd clients/apple && swift test --filter PlaylistViewModelTests`
Expected: 编译失败（`PlaylistViewModel` 未定义）。

- [ ] **Step 3: 实现**

`clients/apple/Sources/MusicApp/Model/PlaylistViewModel.swift`：

```swift
import CoreFFI
import Foundation

@MainActor
final class PlaylistViewModel: ObservableObject {
    @Published private(set) var tree: PlaylistTree?
    @Published private(set) var detail: PlaylistDetail?
    @Published private(set) var errorMessage: String?

    private let client: any MusicClientProviding

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func loadTree() async {
        errorMessage = nil
        do {
            tree = try await client.playlistTree()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func openPlaylist(id: String) async {
        errorMessage = nil
        do {
            detail = try await client.playlistDetail(id: id)
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd clients/apple && swift test --filter PlaylistViewModelTests`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add clients/apple/Sources/MusicApp/Model/PlaylistViewModel.swift clients/apple/Tests/MusicAppTests/PlaylistViewModelTests.swift
git commit -m "feat(mac): 加载歌单树与详情"
```

---

### Task 8: PlaylistViewModel — CRUD 变更与刷新

**Files:**
- Modify: `clients/apple/Sources/MusicApp/Model/PlaylistViewModel.swift`
- Modify: `clients/apple/Tests/MusicAppTests/PlaylistViewModelTests.swift`

**Interfaces:**
- Produces（均在成功后重新 `loadTree()`，失败置 `errorMessage`）：
  `createPlaylist(name:folderID:)`、`createFolder(name:parentID:)`、`rename(playlistID:name:)`、`renameFolder(id:name:)`、`delete(playlistID:)`、`deleteFolder(id:)`、`move(playlistID:folderID:)`、`moveFolder(id:parentID:)`、`addTracks(playlistID:songIDs:)`、`removeTrack(playlistID:index:)`。移曲/加曲若命中当前 `detail`，额外 `openPlaylist(id:)` 刷新详情。

- [ ] **Step 1: 写失败测试**

追加到 `PlaylistViewModelTests`：

```swift
    func testCreatePlaylistCallsClientThenReloadsTree() async {
        let fake = FakePlaylistClient()
        let model = PlaylistViewModel(client: fake)

        await model.createPlaylist(name: "New", folderID: "folder:1")

        XCTAssertTrue(fake.calls.contains("create:New:folder:1"))
        XCTAssertEqual(fake.calls.last, "tree") // 创建后整树刷新
        XCTAssertNil(model.errorMessage)
    }

    func testRemoveTrackReloadsOpenDetail() async {
        let detail = PlaylistDetail(
            playlist: playlistFixture(id: "playlist:5", name: "Mix", folderID: nil),
            tracks: [trackFixture(id: "track:9", title: "Song")]
        )
        let fake = FakePlaylistClient(detail: detail)
        let model = PlaylistViewModel(client: fake)
        await model.openPlaylist(id: "playlist:5")

        await model.removeTrack(playlistID: "playlist:5", index: 0)

        XCTAssertTrue(fake.calls.contains("remove:playlist:5:0"))
        XCTAssertEqual(fake.calls.filter { $0 == "detail:playlist:5" }.count, 2) // 打开 + 移除后刷新
    }

    func testDeleteFolderPropagatesError() async {
        let fake = ThrowingPlaylistClient()
        let model = PlaylistViewModel(client: fake)

        await model.deleteFolder(id: "folder:1")

        XCTAssertNotNil(model.errorMessage)
    }
```

并在文件底部补一个只对某方法抛错的 Fake：

```swift
final class ThrowingPlaylistClient: MusicClientProviding, @unchecked Sendable {
    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user)
    }
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }
    func search(query: String) async throws -> SearchResult { SearchResult(artists: [], albums: [], tracks: []) }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track { throw CocoaError(.featureUnsupported) }
    func updateTags(id: String, update: TagUpdate) async throws {}
    func deleteTrack(id: String) async throws {}
    func moveTrack(id: String, key: String) async throws {}
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func playlistTree() async throws -> PlaylistTree { PlaylistTree(folders: [], playlists: []) }
    func deleteFolder(id: String) async throws { throw CocoaError(.featureUnsupported) }
    // 其余方法走协议默认 featureUnsupported 实现。
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd clients/apple && swift test --filter PlaylistViewModelTests`
Expected: 编译失败（新方法未定义）。

- [ ] **Step 3: 实现**

在 `PlaylistViewModel` 内追加。用一个私有 `perform` 归一化「执行→成功刷新树→失败置错」：

```swift
    func createPlaylist(name: String, folderID: String?) async {
        await mutateTree { try await self.client.createPlaylist(name: name, folderID: folderID, songIDs: []) }
    }
    func createFolder(name: String, parentID: String?) async {
        await mutateTree { _ = try await self.client.createFolder(name: name, parentID: parentID) }
    }
    func rename(playlistID: String, name: String) async {
        await mutateTree { try await self.client.renamePlaylist(id: playlistID, name: name) }
    }
    func renameFolder(id: String, name: String) async {
        await mutateTree { try await self.client.renameFolder(id: id, name: name) }
    }
    func delete(playlistID: String) async {
        await mutateTree { try await self.client.deletePlaylist(id: playlistID) }
    }
    func deleteFolder(id: String) async {
        await mutateTree { try await self.client.deleteFolder(id: id) }
    }
    func move(playlistID: String, folderID: String?) async {
        await mutateTree { try await self.client.movePlaylist(id: playlistID, folderID: folderID) }
    }
    func moveFolder(id: String, parentID: String?) async {
        await mutateTree { try await self.client.moveFolder(id: id, parentID: parentID) }
    }
    func addTracks(playlistID: String, songIDs: [String]) async {
        await mutateDetail(playlistID: playlistID) { try await self.client.addTracks(id: playlistID, songIDs: songIDs) }
    }
    func removeTrack(playlistID: String, index: Int64) async {
        await mutateDetail(playlistID: playlistID) { try await self.client.removeTrackAt(id: playlistID, index: index) }
    }

    private func mutateTree(_ action: () async throws -> Void) async {
        errorMessage = nil
        do {
            try await action()
            await loadTree()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func mutateDetail(playlistID: String, _ action: () async throws -> Void) async {
        errorMessage = nil
        do {
            try await action()
            if detail?.playlist.id == playlistID { await openPlaylist(id: playlistID) }
            await loadTree() // songCount/duration 可能变化
        } catch {
            errorMessage = error.localizedDescription
        }
    }
```

注意：`removeTrack` 测试期望详情被刷新且树也刷新——`mutateDetail` 同时刷新详情与树；测试 `testRemoveTrackReloadsOpenDetail` 断言 `detail:playlist:5` 出现两次（open + 刷新）成立。

- [ ] **Step 4: 跑测试确认通过**

Run: `cd clients/apple && swift test --filter PlaylistViewModelTests`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add clients/apple/Sources/MusicApp/Model/PlaylistViewModel.swift clients/apple/Tests/MusicAppTests/PlaylistViewModelTests.swift
git commit -m "feat(mac): 歌单增删改移动作与刷新"
```

---

### Task 9: 侧栏分区 + 歌单详情视图 + 菜单接线

**Files:**
- Create: `clients/apple/Sources/MusicApp/Views/PlaylistDetailView.swift`
- Modify: `clients/apple/Sources/MusicApp/Views/LibraryView.swift`

**Interfaces:**
- Consumes: `PlaylistViewModel`（Task 7/8）、`MediaViewModel`（既有单曲试听）。
- Produces: `enum SidebarSelection: Hashable { case library, case playlist(String) }`；侧栏「歌单」分区渲染文件夹树 + 叶子；`PlaylistDetailView` 展示曲目与「移出歌单」「加入歌单」。

**说明：** 本任务以「能编译 + 手动冒烟」为准（SwiftUI 视图无单元测试）；ViewModel 行为已由 Task 7/8 覆盖。

- [ ] **Step 1: 建 PlaylistDetailView**

`clients/apple/Sources/MusicApp/Views/PlaylistDetailView.swift`：

```swift
import SwiftUI
import CoreFFI

struct PlaylistDetailView: View {
    let detail: PlaylistDetail
    @ObservedObject var playlists: PlaylistViewModel
    @ObservedObject var media: MediaViewModel

    var body: some View {
        List {
            Section(detail.playlist.name) {
                if detail.tracks.isEmpty {
                    Text("歌单还没有曲目").foregroundStyle(.secondary)
                }
                ForEach(Array(detail.tracks.enumerated()), id: \.element.id) { index, track in
                    HStack {
                        VStack(alignment: .leading) {
                            Text(track.title)
                            Text(track.artist ?? "未知艺人").font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button { Task { await media.play(trackID: track.id) } } label: {
                            Image(systemName: "play.circle")
                        }.buttonStyle(.borderless)
                    }
                    .contextMenu {
                        Button("移出歌单", role: .destructive) {
                            Task { await playlists.removeTrack(playlistID: detail.playlist.id, index: Int64(index)) }
                        }
                    }
                }
            }
        }
        .navigationTitle(detail.playlist.name)
    }
}
```

> 若 `MediaViewModel` 的播放方法名不是 `play(trackID:)`，改为其现有单曲试听入口（读 `MediaViewModel.swift` 确认后对齐）。

- [ ] **Step 2: 改 LibraryView 侧栏与选择模型**

在 `LibraryView.swift`：
1. 顶部（`struct LibraryView` 外）加：

```swift
enum SidebarSelection: Hashable {
    case library
    case playlist(String)
}
```

2. 视图内把 `@State private var selectedAlbumID: String?` 改为 `@State private var selection: SidebarSelection? = .library`，`@State private var selectedAlbumID: String?`（专辑选择保留，用于曲库详情），并新增：

```swift
    @StateObject private var playlists: PlaylistViewModel
```

在 `init` 里补 `_playlists = StateObject(wrappedValue: PlaylistViewModel(client: model.clientForViews))`。

3. 侧栏 `List` 改为分区（`selection: $selection`）：

```swift
List(selection: $selection) {
    Section("资料库") {
        Label("曲库", systemImage: "square.stack").tag(SidebarSelection.library)
    }
    Section("歌单") {
        PlaylistTreeOutline(playlists: playlists)
    }
}
.navigationTitle("音乐")
.toolbar {
    Menu {
        Button("新建歌单") { newPlaylistPrompt = true }
        Button("新建文件夹") { newFolderPrompt = true }
    } label: { Label("新建", systemImage: "plus") }
}
```

4. 详情区按 `selection` 切换：`.library` → 现有专辑列表/搜索（保留原 `albums` List，可作为 library 情形的内容）；`.playlist(id)` → 若 `playlists.detail?.playlist.id == id` 显示 `PlaylistDetailView`，否则 `ProgressView().task { await playlists.openPlaylist(id: id) }`。

5. `.task` 内除 `model.load()` 外追加 `await playlists.loadTree()`。

> 由于既有 `LibraryView` 把专辑列表放在侧栏，此步把专辑列表移到 `.library` 详情区（或保留为 library 子视图）。实现时以「侧栏只放资料库入口 + 歌单树、右侧按 selection 渲染」为准，保持 `workflow.newAlbumIDs` 新增标记逻辑在专辑列表处不丢失。

- [ ] **Step 3: 建歌单树递归视图 + 命名弹窗**

在 `LibraryView.swift` 追加递归 outline 与菜单操作视图：

```swift
struct PlaylistTreeOutline: View {
    @ObservedObject var playlists: PlaylistViewModel

    var body: some View {
        if let tree = playlists.tree {
            let roots = tree.folders.filter { $0.parentId == nil }
            ForEach(roots, id: \.id) { folder in
                FolderNode(folder: folder, tree: tree, playlists: playlists)
            }
            ForEach(tree.playlists.filter { $0.folderId == nil }, id: \.id) { playlist in
                PlaylistLeaf(playlist: playlist, playlists: playlists)
            }
        } else {
            Text("加载中…").foregroundStyle(.secondary)
        }
    }
}

struct FolderNode: View {
    let folder: PlaylistFolder
    let tree: PlaylistTree
    @ObservedObject var playlists: PlaylistViewModel

    var body: some View {
        DisclosureGroup {
            ForEach(tree.folders.filter { $0.parentId == folder.id }, id: \.id) { child in
                FolderNode(folder: child, tree: tree, playlists: playlists)
            }
            ForEach(tree.playlists.filter { $0.folderId == folder.id }, id: \.id) { playlist in
                PlaylistLeaf(playlist: playlist, playlists: playlists)
            }
        } label: {
            Label(folder.name, systemImage: "folder")
                .contextMenu {
                    Button("重命名") { /* 触发命名弹窗，见下 */ }
                    Button("删除", role: .destructive) { Task { await playlists.deleteFolder(id: folder.id) } }
                }
        }
    }
}

struct PlaylistLeaf: View {
    let playlist: Playlist
    @ObservedObject var playlists: PlaylistViewModel

    var body: some View {
        Label(playlist.name, systemImage: "music.note.list")
            .tag(SidebarSelection.playlist(playlist.id))
            .contextMenu {
                Button("重命名") { /* 命名弹窗 */ }
                Button("删除", role: .destructive) { Task { await playlists.delete(playlistID: playlist.id) } }
            }
    }
}
```

重命名/新建使用 `.alert` + `@State` 文本绑定；删除文件夹/歌单用 `confirmationDialog` 二次确认。为控制篇幅，命名与确认弹窗集中放在 `LibraryView` 顶层，用 `@State var pendingRename: SidebarSelection?`、`@State var renameText = ""` 等驱动，节点内菜单只设置这些 `@State`。实现者按 SwiftUI 常规 `.alert(_:isPresented:)` 接线，提交时调用对应 `playlists.rename(...)`/`renameFolder(...)`/`createPlaylist(...)`/`createFolder(...)`。

- [ ] **Step 4: 编译校验**

Run: `cd clients/apple && swift build`
Expected: 编译通过。

- [ ] **Step 5: 手动冒烟（需运行中的服务）**

Run: `scripts/run-mac-client.sh`（按其提示连服务）。核对：侧栏出现「资料库/歌单」分区；「+」菜单可新建歌单与文件夹并即时出现在树中；右键可重命名/删除（删除有二次确认）；点歌单进入详情看到曲目；「移出歌单」后曲目消失。

- [ ] **Step 6: 提交**

```bash
git add clients/apple/Sources/MusicApp/Views/PlaylistDetailView.swift clients/apple/Sources/MusicApp/Views/LibraryView.swift
git commit -m "feat(mac): 侧栏多级歌单导航与管理"
```

---

### Task 10: 「加入歌单」子菜单 + 移动目标选择 + 端到端冒烟

**Files:**
- Modify: `clients/apple/Sources/MusicApp/Views/MediaDetailView.swift`
- Modify: `clients/apple/Sources/MusicApp/Views/LibraryView.swift`

**Interfaces:**
- Consumes: `PlaylistViewModel.tree`（列出目标歌单/文件夹）、`addTracks`、`move`/`moveFolder`。
- Produces: 专辑与歌单曲目行的「加入歌单 ▸」子菜单；树节点「移动到…」子菜单（列出文件夹 + 「根目录」）。

- [ ] **Step 1: 专辑曲目行「加入歌单」**

读 `MediaDetailView.swift` 确认曲目行渲染处，为每行 `.contextMenu` 增加子菜单。`MediaDetailView` 需能访问 `PlaylistViewModel`：给它加 `@ObservedObject var playlists: PlaylistViewModel` 参数（在 `LibraryView` 构造 `MediaDetailView` 处传入 `playlists`）。子菜单：

```swift
Menu("加入歌单") {
    ForEach(playlists.tree?.playlists ?? [], id: \.id) { pl in
        Button(pl.name) { Task { await playlists.addTracks(playlistID: pl.id, songIDs: [track.id]) } }
    }
}
```

- [ ] **Step 2: 树节点「移动到…」**

在 `FolderNode`/`PlaylistLeaf` 的 `contextMenu` 增加：

```swift
Menu("移动到…") {
    Button("根目录") { Task { await playlists.move(playlistID: playlist.id, folderID: nil) } } // 叶子用 move；文件夹节点用 moveFolder(id:parentID:)
    ForEach(playlists.tree?.folders ?? [], id: \.id) { target in
        Button(target.name) { Task { await playlists.move(playlistID: playlist.id, folderID: target.id) } }
    }
}
```

文件夹节点对应改用 `moveFolder(id:parentID:)`；移动到自身/子孙由服务端拒绝并经 `errorMessage` 呈现。

- [ ] **Step 3: 编译校验**

Run: `cd clients/apple && swift build && swift test`
Expected: 编译通过、既有 XCTest 全绿。

- [ ] **Step 4: 端到端冒烟（需运行中的服务）**

Run: `scripts/run-mac-client.sh`。完整走查：建文件夹 → 建歌单（选择落入该文件夹）→ 在专辑详情右键「加入歌单」把曲目加进去 → 打开歌单看到曲目 → 「移出歌单」 → 把歌单「移动到…根目录」 → 删除歌单与文件夹。确认每步 UI 即时反映、无报错；试听可正常播放。

- [ ] **Step 5: 提交**

```bash
git add clients/apple/Sources/MusicApp/Views/MediaDetailView.swift clients/apple/Sources/MusicApp/Views/LibraryView.swift
git commit -m "feat(mac): 曲目加入歌单与节点移动"
```

---

### Task 11: 全量校验 + 文档

**Files:**
- Modify: `docs/superpowers/specs/2026-07-12-mac-multilevel-playlist-design.md`（如实现与设计有偏差，同步）

- [ ] **Step 1: Rust 全量**

Run: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`
Expected: 全绿。

- [ ] **Step 2: Swift 全量**

Run: `cd clients/apple && swift test`
Expected: 全绿。

- [ ] **Step 3: 一键启动脚本测试**

Run: `scripts/tests/run-mac-client-test.sh`
Expected: 通过。

- [ ] **Step 4: 同步文档（如有偏差）并提交**

若实现与 spec 有出入，更新 spec 对应段落。

```bash
git add -A
git commit -m "docs(mac): 校准多级歌单实现说明"
```

---

## Self-Review

- **Spec 覆盖**：§4 core 全部方法 → Task 1–5；`ffi_types` 桥接 → Task 1；create+move 组合 → Task 3；§5 侧栏分区/`PlaylistViewModel`/`PlaylistDetailView`/菜单驱动 CRUD/加入歌单 → Task 6–10；§6 错误处理 → ViewModel `errorMessage`（Task 7/8）+ 服务端环检测透传（Task 5/10）；§7 测试 → core 集成测试（Task 1–5）、XCTest（Task 7/8）、端到端冒烟（Task 9/10/11）。无遗漏。
- **占位符**：无 TBD/TODO；所有代码步骤给出完整代码。视图弹窗接线以 SwiftUI 常规写法描述（`.alert`/`confirmationDialog`），非产品逻辑，风险低。
- **类型一致**：core `PlaylistTree{folders,playlists}`、`PlaylistDetail{playlist,tracks}` 全程一致；Swift 协议方法名（`playlistTree`、`playlistDetail`、`createPlaylist`、`createFolder`、`movePlaylist`、`moveFolder` 等）在协议、Fake、ViewModel、视图中一致；UniFFI 生成参数名 `folderId`/`songIds`/`parentId` 已在桥接层显式对齐。
