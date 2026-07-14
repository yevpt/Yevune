import SwiftUI
import YevuneCoreFFI
import UniformTypeIdentifiers

enum SidebarSelection: Hashable {
    case library
    case playlist(String)
    case adminUsers
    case adminRoles
    case adminAccess
}

/// 重命名目标：区分歌单与文件夹。
enum RenameTarget: Hashable {
    case playlist(String)
    case folder(String)
}

/// 删除目标：区分歌单与文件夹，供二次确认使用。
enum DeleteTarget: Hashable {
    case playlist(String)
    case folder(String)
}

struct LibraryView: View {
    @ObservedObject var model: LibraryViewModel
    let session: SessionValue
    @State private var query = ""
    @State private var selection: SidebarSelection? = .library
    @State private var selectedAlbumID: String?
    @StateObject private var media: MediaViewModel
    @StateObject private var workflow: LibraryWorkflowViewModel
    @StateObject private var playlists: PlaylistViewModel
    @StateObject private var admin: AdminViewModel
    @StateObject private var access: AccessControlViewModel
    @State private var accessTarget: AccessScopeTarget?
    @State private var importing = false
    @State private var isDropTargeted = false

    // 新建 / 重命名 / 删除 弹窗状态（集中在顶层驱动，节点内菜单只设置这些 @State）。
    @State private var newPlaylistPrompt = false
    @State private var newFolderPrompt = false
    @State private var createText = ""
    @State private var renameTarget: RenameTarget?
    @State private var renameText = ""
    @State private var deleteTarget: DeleteTarget?

    init(model: LibraryViewModel, session: SessionValue) {
        self.model = model
        self.session = session
        _media = StateObject(wrappedValue: MediaViewModel(client: model.clientForViews))
        _workflow = StateObject(wrappedValue: LibraryWorkflowViewModel(client: model.clientForViews, library: model))
        _playlists = StateObject(wrappedValue: PlaylistViewModel(client: model.clientForViews))
        _admin = StateObject(wrappedValue: AdminViewModel(currentUsername: session.user, client: model.clientForViews))
        _access = StateObject(wrappedValue: AccessControlViewModel(client: model.clientForViews))
    }

    var body: some View {
        NavigationSplitView {
            List(selection: $selection) {
                Section("资料库") {
                    Label("曲库", systemImage: "square.stack").tag(SidebarSelection.library)
                }
                Section("歌单") {
                    PlaylistTreeOutline(
                        playlists: playlists,
                        onRename: { target, currentName in
                            renameTarget = target
                            renameText = currentName
                        },
                        onDelete: { target in deleteTarget = target }
                    )
                }
                if AccessManagementPolicy.allowsEntry(isAdmin: session.admin) {
                    Section("管理") {
                        Label("用户", systemImage: "person.2")
                            .tag(SidebarSelection.adminUsers)
                        Label("角色", systemImage: "person.badge.key")
                            .tag(SidebarSelection.adminRoles)
                        Label("访问控制", systemImage: "eye.badge")
                            .tag(SidebarSelection.adminAccess)
                    }
                }
                if let playlistError = playlists.errorMessage {
                    Text(playlistError).foregroundStyle(.red).font(.caption)
                }
            }
            .navigationTitle("音乐")
            .toolbar {
                Menu {
                    Button("新建歌单") { createText = ""; newPlaylistPrompt = true }
                    Button("新建文件夹") { createText = ""; newFolderPrompt = true }
                } label: { Label("新建", systemImage: "plus") }
            }
        } detail: {
            detailContent
        }
        .task {
            async let libraryLoad: Void = model.load()
            async let playlistLoad: Void = playlists.loadTree()
            if AccessManagementPolicy.allowsEntry(isAdmin: session.admin) {
                async let accessLoad: Void = access.load()
                _ = await (libraryLoad, playlistLoad, accessLoad)
            } else {
                _ = await (libraryLoad, playlistLoad)
            }
        }
        .toolbar {
            Button { importing = true } label: { Label("导入音乐", systemImage: "plus") }
            Button { Task { await workflow.scanLibrary() } } label: { Label("扫描曲库", systemImage: "arrow.clockwise") }
            Button { workflow.isDrawerPresented.toggle() } label: { Label("任务", systemImage: "tray.full") }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.audio], allowsMultipleSelection: true) { result in
            if case let .success(urls) = result { Task { await workflow.importFiles(urls) } }
        }
        .dropDestination(for: URL.self) { urls, _ in
            Task { await workflow.importFiles(urls) }; return true
        } isTargeted: { isDropTargeted = $0 }
        .overlay { if isDropTargeted { RoundedRectangle(cornerRadius: 18).fill(.indigo.opacity(0.2)).overlay { Label("松开以导入音乐", systemImage: "square.and.arrow.down").font(.title2.bold()) } } }
        .safeAreaInset(edge: .bottom, spacing: 0) { if workflow.isDrawerPresented { TaskDrawerView(model: workflow).frame(maxHeight: 300) } }
        .alert("新建歌单", isPresented: $newPlaylistPrompt) {
            TextField("歌单名称", text: $createText)
            Button("取消", role: .cancel) {}
            Button("创建") {
                let name = createText.trimmingCharacters(in: .whitespacesAndNewlines)
                if !name.isEmpty { Task { await playlists.createPlaylist(name: name, folderID: nil) } }
            }
        }
        .alert("新建文件夹", isPresented: $newFolderPrompt) {
            TextField("文件夹名称", text: $createText)
            Button("取消", role: .cancel) {}
            Button("创建") {
                let name = createText.trimmingCharacters(in: .whitespacesAndNewlines)
                if !name.isEmpty { Task { await playlists.createFolder(name: name, parentID: nil) } }
            }
        }
        .alert("重命名", isPresented: renameIsPresented) {
            TextField("新名称", text: $renameText)
            Button("取消", role: .cancel) { renameTarget = nil }
            Button("确定") {
                let name = renameText.trimmingCharacters(in: .whitespacesAndNewlines)
                if let target = renameTarget, !name.isEmpty {
                    Task {
                        switch target {
                        case .playlist(let id): await playlists.rename(playlistID: id, name: name)
                        case .folder(let id): await playlists.renameFolder(id: id, name: name)
                        }
                    }
                }
                renameTarget = nil
            }
        }
        .confirmationDialog("确认删除？此操作不可撤销。", isPresented: deleteIsPresented, titleVisibility: .visible) {
            Button("删除", role: .destructive) {
                if let target = deleteTarget {
                    Task {
                        switch target {
                        case .playlist(let id): await playlists.delete(playlistID: id)
                        case .folder(let id): await playlists.deleteFolder(id: id)
                        }
                    }
                }
                deleteTarget = nil
            }
            Button("取消", role: .cancel) { deleteTarget = nil }
        }
        .sheet(isPresented: accessTargetIsPresented) {
            if let target = accessTarget {
                switch AccessManagementPolicy.editorPresentation(
                    hasLoadedSuccessfully: access.hasLoadedSuccessfully,
                    isLoading: access.isLoading,
                    errorMessage: access.errorMessage
                ) {
                case .editor:
                    AccessRuleEditorView(target: target, model: access) {
                        accessTarget = nil
                    }
                case .loading:
                    ProgressView("正在加载可见范围…")
                        .padding(32)
                case .unavailable(let message):
                    VStack(spacing: 16) {
                        ContentUnavailableView {
                            Label("无法编辑可见范围", systemImage: "wifi.exclamationmark")
                        } description: {
                            Text(message)
                        }
                        Button("重新加载") {
                            Task { await access.load() }
                        }
                        .buttonStyle(.borderedProminent)
                    }
                    .padding(32)
                }
            }
        }
    }

    @ViewBuilder private var detailContent: some View {
        switch selection {
        case .adminUsers:
            AdminUsersView(model: admin, access: access)
        case .adminRoles:
            AdminRolesView(model: admin, access: access)
        case .adminAccess:
            AdminAccessRulesView(model: access)
        case .playlist(let id):
            if let detail = playlists.detail, detail.playlist.id == id {
                PlaylistDetailView(detail: detail, playlists: playlists, media: media)
            } else {
                ProgressView().task(id: id) { await playlists.openPlaylist(id: id) }
            }
        case .library, .none:
            libraryDetail
        }
    }

    /// 资料库详情：浏览工具条 + 专辑网格/列表（含「新增」标记）+ 搜索 + 专辑详情，保留原有全部行为。
    @ViewBuilder private var libraryDetail: some View {
        VStack(spacing: 0) {
            browseToolbar
            Divider()
            HStack(spacing: 0) {
                Group {
                    if model.viewMode == .grid {
                        AlbumGridView(
                            albums: model.albums,
                            client: model.clientForViews,
                            newAlbumIDs: workflow.newAlbumIDs,
                            onSelect: { selectedAlbumID = $0.id },
                            onManageAccess: manageAccess
                        )
                    } else {
                        List(model.albums, id: \.id, selection: $selectedAlbumID) { album in
                            VStack(alignment: .leading, spacing: 3) {
                                HStack {
                                    Text(album.name).font(.headline)
                                    if workflow.newAlbumIDs.contains(album.id) {
                                        Text("新增").font(.caption2).padding(.horizontal, 5).background(.green.opacity(0.2), in: Capsule())
                                    }
                                }
                                Text(album.artist ?? "未知艺人")
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                            }
                            .tag(album.id)
                            .contextMenu {
                                if let manageAccess {
                                    Button("设置专辑可见范围") {
                                        manageAccess(.fromAlbum(album))
                                    }
                                }
                            }
                        }
                    }
                }
                .frame(minWidth: 260, idealWidth: 340, maxWidth: model.viewMode == .grid ? .infinity : 320)

                Divider()

                if let selection = model.album(id: selectedAlbumID) {
                    MediaDetailView(
                        album: selection,
                        model: media,
                        playlists: playlists,
                        onManageAccess: manageAccess
                    )
                } else {
                    VStack(spacing: 18) {
                        TextField("搜索艺人、专辑或曲目", text: $query)
                            .textFieldStyle(.roundedBorder)
                            .onSubmit { Task { await model.search(query: query) } }
                        if let result = model.searchResult {
                            List(result.albums, id: \.id) { album in
                                Text(album.name)
                            }
                        } else if model.isLoading {
                            ProgressView("正在加载曲库…")
                        } else if let errorMessage = model.errorMessage {
                            Text(errorMessage).foregroundStyle(.red)
                        } else {
                            Text("选择专辑以查看曲目")
                                .foregroundStyle(.secondary)
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .padding()
                }
            }
        }
        .task { await model.loadGenres() }
    }

    @ViewBuilder private var browseToolbar: some View {
        HStack(spacing: 16) {
            Picker("排序", selection: $model.sort) {
                Text("最近入库").tag(AlbumSort.newest)
                Text("按专辑名").tag(AlbumSort.alphabeticalByName)
                Text("按艺人名").tag(AlbumSort.alphabeticalByArtist)
                Text("最常播放").tag(AlbumSort.frequent)
                Text("最近播放").tag(AlbumSort.recent)
            }
            .frame(maxWidth: 160)
            .disabled(model.genreFilter != nil || model.yearFilterEnabled)

            Picker("流派", selection: genreBinding) {
                Text("全部").tag(String?.none)
                ForEach(model.genres, id: \.value) { genre in
                    Text(genre.value).tag(String?.some(genre.value))
                }
            }
            .frame(maxWidth: 160)

            Toggle("按年份", isOn: $model.yearFilterEnabled)
            if model.yearFilterEnabled {
                Stepper("从 \(model.fromYear)", value: yearBinding(\.fromYear), in: 1900...2100)
                Stepper("到 \(model.toYear)", value: yearBinding(\.toYear), in: 1900...2100)
            }

            Spacer()

            if AccessManagementPolicy.allowsEntry(isAdmin: session.admin), let genre = model.genreFilter {
                Button {
                    accessTarget = .fromGenre(genre)
                } label: {
                    Label("可见范围", systemImage: "eye")
                }
            }

            Picker("视图", selection: $model.viewMode) {
                ForEach(LibraryViewMode.allCases) { mode in
                    Text(mode.rawValue).tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .frame(maxWidth: 160)
        }
        .padding()
        .onChange(of: model.sort) { _, _ in Task { await model.load() } }
        .onChange(of: model.genreFilter) { _, _ in Task { await model.load() } }
        .onChange(of: model.yearFilterEnabled) { _, _ in Task { await model.load() } }
        .onChange(of: model.fromYear) { _, _ in if model.yearFilterEnabled { Task { await model.load() } } }
        .onChange(of: model.toYear) { _, _ in if model.yearFilterEnabled { Task { await model.load() } } }
    }

    private var genreBinding: Binding<String?> {
        Binding(get: { model.genreFilter }, set: { model.genreFilter = $0 })
    }

    private func yearBinding(_ keyPath: ReferenceWritableKeyPath<LibraryViewModel, UInt32>) -> Binding<UInt32> {
        Binding(get: { model[keyPath: keyPath] }, set: { model[keyPath: keyPath] = $0 })
    }

    private var renameIsPresented: Binding<Bool> {
        Binding(get: { renameTarget != nil }, set: { if !$0 { renameTarget = nil } })
    }

    private var deleteIsPresented: Binding<Bool> {
        Binding(get: { deleteTarget != nil }, set: { if !$0 { deleteTarget = nil } })
    }

    private var accessTargetIsPresented: Binding<Bool> {
        Binding(get: { accessTarget != nil }, set: { if !$0 { accessTarget = nil } })
    }

    private var manageAccess: ((AccessScopeTarget) -> Void)? {
        AccessManagementPolicy.allowsEntry(isAdmin: session.admin) ? { accessTarget = $0 } : nil
    }
}

// MARK: - 歌单树递归视图

struct PlaylistTreeOutline: View {
    @ObservedObject var playlists: PlaylistViewModel
    let onRename: (RenameTarget, String) -> Void
    let onDelete: (DeleteTarget) -> Void

    var body: some View {
        if let tree = playlists.tree {
            let roots = tree.folders.filter { $0.parentId == nil }
            ForEach(roots, id: \.id) { folder in
                FolderNode(folder: folder, tree: tree, playlists: playlists, onRename: onRename, onDelete: onDelete)
            }
            ForEach(tree.playlists.filter { $0.folderId == nil }, id: \.id) { playlist in
                PlaylistLeaf(playlist: playlist, playlists: playlists, onRename: onRename, onDelete: onDelete)
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
    let onRename: (RenameTarget, String) -> Void
    let onDelete: (DeleteTarget) -> Void

    var body: some View {
        DisclosureGroup {
            ForEach(tree.folders.filter { $0.parentId == folder.id }, id: \.id) { child in
                FolderNode(folder: child, tree: tree, playlists: playlists, onRename: onRename, onDelete: onDelete)
            }
            ForEach(tree.playlists.filter { $0.folderId == folder.id }, id: \.id) { playlist in
                PlaylistLeaf(playlist: playlist, playlists: playlists, onRename: onRename, onDelete: onDelete)
            }
        } label: {
            Label(folder.name, systemImage: "folder")
                .contextMenu {
                    Button("重命名") { onRename(.folder(folder.id), folder.name) }
                    Menu("移动到…") {
                        Button("根目录") { Task { await playlists.moveFolder(id: folder.id, parentID: nil) } }
                        ForEach(playlists.tree?.folders ?? [], id: \.id) { target in
                            Button(target.name) { Task { await playlists.moveFolder(id: folder.id, parentID: target.id) } }
                        }
                    }
                    Button("删除", role: .destructive) { onDelete(.folder(folder.id)) }
                }
        }
    }
}

struct PlaylistLeaf: View {
    let playlist: Playlist
    @ObservedObject var playlists: PlaylistViewModel
    let onRename: (RenameTarget, String) -> Void
    let onDelete: (DeleteTarget) -> Void

    var body: some View {
        Label(playlist.name, systemImage: "music.note.list")
            .tag(SidebarSelection.playlist(playlist.id))
            .contextMenu {
                Button("重命名") { onRename(.playlist(playlist.id), playlist.name) }
                Menu("移动到…") {
                    Button("根目录") { Task { await playlists.move(playlistID: playlist.id, folderID: nil) } }
                    ForEach(playlists.tree?.folders ?? [], id: \.id) { target in
                        Button(target.name) { Task { await playlists.move(playlistID: playlist.id, folderID: target.id) } }
                    }
                }
                Button("删除", role: .destructive) { onDelete(.playlist(playlist.id)) }
            }
    }
}
