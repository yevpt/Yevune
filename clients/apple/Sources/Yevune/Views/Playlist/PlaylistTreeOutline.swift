import SwiftUI
import YevuneCoreFFI

struct PlaylistTreeOutline: View {
    @ObservedObject var playlists: PlaylistViewModel
    let onRename: (RenameTarget, String) -> Void
    let onDelete: (DeleteTarget) -> Void

    var body: some View {
        if let tree = playlists.tree {
            let roots = tree.folders.filter { $0.parentId == nil }
            ForEach(roots, id: \.id) { folder in
                FolderNode(
                    folder: folder,
                    tree: tree,
                    playlists: playlists,
                    onRename: onRename,
                    onDelete: onDelete
                )
            }
            ForEach(tree.playlists.filter { $0.folderId == nil }, id: \.id) { playlist in
                PlaylistLeaf(
                    playlist: playlist,
                    playlists: playlists,
                    onRename: onRename,
                    onDelete: onDelete
                )
            }
        } else {
            Text("加载中…").foregroundStyle(.secondary)
        }
    }
}

private struct FolderNode: View {
    let folder: PlaylistFolder
    let tree: PlaylistTree
    @ObservedObject var playlists: PlaylistViewModel
    let onRename: (RenameTarget, String) -> Void
    let onDelete: (DeleteTarget) -> Void

    var body: some View {
        DisclosureGroup {
            ForEach(tree.folders.filter { $0.parentId == folder.id }, id: \.id) { child in
                FolderNode(
                    folder: child,
                    tree: tree,
                    playlists: playlists,
                    onRename: onRename,
                    onDelete: onDelete
                )
            }
            ForEach(tree.playlists.filter { $0.folderId == folder.id }, id: \.id) { playlist in
                PlaylistLeaf(
                    playlist: playlist,
                    playlists: playlists,
                    onRename: onRename,
                    onDelete: onDelete
                )
            }
        } label: {
            Label(folder.name, systemImage: "folder")
                .contextMenu {
                    Button("重命名") { onRename(.folder(folder.id), folder.name) }
                    Menu("移动到…") {
                        Button("根目录") {
                            Task { await playlists.moveFolder(id: folder.id, parentID: nil) }
                        }
                        ForEach(playlists.tree?.folders ?? [], id: \.id) { target in
                            Button(target.name) {
                                Task { await playlists.moveFolder(id: folder.id, parentID: target.id) }
                            }
                        }
                    }
                    Button("删除", role: .destructive) { onDelete(.folder(folder.id)) }
                }
        }
    }
}

private struct PlaylistLeaf: View {
    let playlist: Playlist
    @ObservedObject var playlists: PlaylistViewModel
    let onRename: (RenameTarget, String) -> Void
    let onDelete: (DeleteTarget) -> Void
    @State private var isDropTargeted = false

    var body: some View {
        HStack(spacing: 8) {
            Label(playlist.name, systemImage: isDropTargeted ? "plus.circle.fill" : "music.note.list")
                .lineLimit(1)
            Spacer(minLength: 4)
            Text("\(playlist.songCount)")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
        }
            .padding(.vertical, 3)
            .padding(.horizontal, 5)
            .background(
                isDropTargeted ? Color.accentColor.opacity(0.18) : .clear,
                in: RoundedRectangle(cornerRadius: 6, style: .continuous)
            )
            .contentShape(Rectangle())
            .accessibilityElement(children: .combine)
            .accessibilityLabel("歌单 \(playlist.name)，\(playlist.songCount) 首歌曲")
            .tag(SidebarSelection.playlist(playlist.id))
            .help("拖入歌曲以加入“\(playlist.name)”")
            .contextMenu {
                Button("重命名") { onRename(.playlist(playlist.id), playlist.name) }
                Menu("移动到…") {
                    Button("根目录") {
                        Task { await playlists.move(playlistID: playlist.id, folderID: nil) }
                    }
                    ForEach(playlists.tree?.folders ?? [], id: \.id) { target in
                        Button(target.name) {
                            Task { await playlists.move(playlistID: playlist.id, folderID: target.id) }
                        }
                    }
                }
                Button("删除", role: .destructive) { onDelete(.playlist(playlist.id)) }
            }
            .dropDestination(for: TrackDragPayload.self) { payloads, _ in
                guard let trackIDs = TrackDragPolicy.acceptedTrackIDs(
                    from: payloads,
                    isMutating: playlists.isMutating
                ) else { return false }
                Task {
                    _ = await playlists.addTracks(
                        playlistID: playlist.id,
                        songIDs: trackIDs
                    )
                }
                return true
            } isTargeted: { isDropTargeted = $0 && !playlists.isMutating }
    }
}
