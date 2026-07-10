-- 初始模式（设计文档 §6）。SQLite 本地磁盘 + WAL；FTS5 支撑 search3。
-- 标识符主键用 INTEGER（省空间、rowid 高效），仓储层转为 DTO 的不透明 String。

-- ── 用户与角色 ────────────────────────────────────────────────
CREATE TABLE users (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL UNIQUE,
    password_enc TEXT NOT NULL,             -- 可逆加密（支持 Subsonic token 校验，见 §10）
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE roles (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL UNIQUE,
    is_builtin INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE user_roles (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id INTEGER NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);

-- ── 媒体：艺人 / 专辑 / 曲目 ──────────────────────────────────
CREATE TABLE artists (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    name      TEXT NOT NULL UNIQUE,
    sort_name TEXT,
    mbid      TEXT,
    cover_key TEXT
);

CREATE TABLE albums (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    name      TEXT NOT NULL,
    artist_id INTEGER REFERENCES artists(id) ON DELETE SET NULL,
    year      INTEGER,
    genre     TEXT,
    cover_key TEXT,
    added_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (name, artist_id)
);
CREATE INDEX idx_albums_artist ON albums(artist_id);

CREATE TABLE tracks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    title        TEXT NOT NULL,
    album_id     INTEGER REFERENCES albums(id) ON DELETE SET NULL,
    artist_id    INTEGER REFERENCES artists(id) ON DELETE SET NULL,
    disc_no      INTEGER,
    track_no     INTEGER,
    year         INTEGER,
    genre        TEXT,
    duration     INTEGER,                   -- 秒
    codec        TEXT,
    bitrate      INTEGER,                   -- kbps
    size         INTEGER,                   -- 字节
    object_key   TEXT NOT NULL UNIQUE,      -- Garage 原始文件键（唯一源）
    etag         TEXT,                      -- 变更检测
    content_hash TEXT,
    replaygain   REAL,
    added_at     TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_tracks_album ON tracks(album_id);
CREATE INDEX idx_tracks_artist ON tracks(artist_id);

-- ── 标注与标签覆盖层 ─────────────────────────────────────────
CREATE TABLE annotations (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_type   TEXT NOT NULL,              -- track/album/artist
    item_id     INTEGER NOT NULL,
    starred_at  TEXT,
    play_count  INTEGER NOT NULL DEFAULT 0,
    last_played TEXT,
    rating      INTEGER,
    PRIMARY KEY (user_id, item_type, item_id)
);

CREATE TABLE tag_overrides (
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    field    TEXT NOT NULL,
    value    TEXT,
    PRIMARY KEY (track_id, field)
);

-- ── 多级歌单（每用户一棵树，owner_id 隔离）────────────────────
CREATE TABLE playlist_folders (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_id  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name      TEXT NOT NULL,
    parent_id INTEGER REFERENCES playlist_folders(id) ON DELETE CASCADE,
    position  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_folders_owner ON playlist_folders(owner_id);
CREATE INDEX idx_folders_parent ON playlist_folders(parent_id);

CREATE TABLE playlists (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_id   INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    comment    TEXT,
    folder_id  INTEGER REFERENCES playlist_folders(id) ON DELETE SET NULL,
    position   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    changed_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_playlists_owner ON playlists(owner_id);
CREATE INDEX idx_playlists_folder ON playlists(folder_id);

CREATE TABLE playlist_tracks (
    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, position)
);
CREATE INDEX idx_pltracks_track ON playlist_tracks(track_id);

-- ── 曲库访问控制（默认开放，仅为被限制内容存规则）───────────────
CREATE TABLE access_rules (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    scope_type TEXT NOT NULL,               -- track/album/artist/genre
    scope_id   TEXT NOT NULL,               -- 整数 id 或流派名，统一 TEXT
    created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (scope_type, scope_id)
);
CREATE INDEX idx_access_scope ON access_rules(scope_type, scope_id);

CREATE TABLE access_rule_grants (
    rule_id        INTEGER NOT NULL REFERENCES access_rules(id) ON DELETE CASCADE,
    principal_type TEXT NOT NULL,           -- user/role
    principal_id   INTEGER NOT NULL,
    PRIMARY KEY (rule_id, principal_type, principal_id)
);

-- ── 转码缓存登记（本体在 Garage）──────────────────────────────
CREATE TABLE transcode_cache (
    track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    format      TEXT NOT NULL,
    bitrate     INTEGER NOT NULL,
    object_key  TEXT NOT NULL,
    size        INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    last_access TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (track_id, format, bitrate)
);

-- ── 扫描进度（单行，断点续扫）─────────────────────────────────
CREATE TABLE scan_state (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    last_scan_at TEXT,
    cursor       TEXT
);
INSERT INTO scan_state (id) VALUES (1);

-- ── 全文搜索（FTS5 trigram，支持中文/任意子串）────────────────
CREATE VIRTUAL TABLE search_fts USING fts5(
    kind UNINDEXED,                         -- track/album/artist
    ref_id UNINDEXED,                       -- 对应表主键
    name,                                   -- 可搜索文本
    tokenize = 'trigram'
);

-- 触发器：随 artists/albums/tracks 增删改同步 FTS 索引
CREATE TRIGGER artists_ai AFTER INSERT ON artists BEGIN
    INSERT INTO search_fts(kind, ref_id, name) VALUES ('artist', NEW.id, NEW.name);
END;
CREATE TRIGGER artists_ad AFTER DELETE ON artists BEGIN
    DELETE FROM search_fts WHERE kind = 'artist' AND ref_id = OLD.id;
END;
CREATE TRIGGER artists_au AFTER UPDATE OF name ON artists BEGIN
    UPDATE search_fts SET name = NEW.name WHERE kind = 'artist' AND ref_id = OLD.id;
END;

CREATE TRIGGER albums_ai AFTER INSERT ON albums BEGIN
    INSERT INTO search_fts(kind, ref_id, name) VALUES ('album', NEW.id, NEW.name);
END;
CREATE TRIGGER albums_ad AFTER DELETE ON albums BEGIN
    DELETE FROM search_fts WHERE kind = 'album' AND ref_id = OLD.id;
END;
CREATE TRIGGER albums_au AFTER UPDATE OF name ON albums BEGIN
    UPDATE search_fts SET name = NEW.name WHERE kind = 'album' AND ref_id = OLD.id;
END;

CREATE TRIGGER tracks_ai AFTER INSERT ON tracks BEGIN
    INSERT INTO search_fts(kind, ref_id, name) VALUES ('track', NEW.id, NEW.title);
END;
CREATE TRIGGER tracks_ad AFTER DELETE ON tracks BEGIN
    DELETE FROM search_fts WHERE kind = 'track' AND ref_id = OLD.id;
END;
CREATE TRIGGER tracks_au AFTER UPDATE OF title ON tracks BEGIN
    UPDATE search_fts SET name = NEW.title WHERE kind = 'track' AND ref_id = OLD.id;
END;
