import SwiftUI
import YevuneCoreFFI
import UniformTypeIdentifiers

func coverArtID(for album: Album) -> String? {
    album.coverArt
}

func loadCoverURL(for album: Album, client: any MusicClientProviding) async -> URL? {
    guard let coverArtID = coverArtID(for: album),
          let urlString = try? await client.coverArtURL(id: coverArtID, size: 300) else {
        return nil
    }
    return URL(string: urlString)
}

func playbackTime(_ seconds: UInt32) -> String {
    playbackTime(TimeInterval(seconds))
}

func playbackTime(_ seconds: TimeInterval) -> String {
    guard seconds.isFinite, seconds > 0 else { return "0:00" }
    let total = Int(seconds.rounded(.down))
    return String(format: "%d:%02d", total / 60, total % 60)
}

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
    let client: any MusicClientProviding
    @ObservedObject var browse: LibraryBrowseViewModel
    @ObservedObject var search: LibrarySearchViewModel
    @ObservedObject var artistDetail: ArtistDetailViewModel
    @ObservedObject var workflow: LibraryWorkflowViewModel
    @ObservedObject var playback: PlaybackController
    let session: SessionValue
    let onLogout: () -> Void
    @State private var selection: SidebarSelection? = .library
    @StateObject private var media: MediaViewModel
    @StateObject private var playlists: PlaylistViewModel
    @StateObject private var admin: AdminViewModel
    @StateObject private var access: AccessControlViewModel
    @StateObject private var lyrics: LyricsViewModel
    @State private var accessTarget: AccessScopeTarget?
    @State private var importing = false
    @State private var isDropTargeted = false
    @State private var isNowPlayingPresented = false

    // 新建 / 重命名 / 删除 弹窗状态（集中在顶层驱动，节点内菜单只设置这些 @State）。
    @State private var newPlaylistPrompt = false
    @State private var newFolderPrompt = false
    @State private var createText = ""
    @State private var renameTarget: RenameTarget?
    @State private var renameText = ""
    @State private var deleteTarget: DeleteTarget?

    init(
        client: any MusicClientProviding,
        browse: LibraryBrowseViewModel,
        search: LibrarySearchViewModel,
        artistDetail: ArtistDetailViewModel,
        workflow: LibraryWorkflowViewModel,
        session: SessionValue,
        playback: PlaybackController,
        onLogout: @escaping () -> Void
    ) {
        self.client = client
        self.browse = browse
        self.search = search
        self.artistDetail = artistDetail
        self.workflow = workflow
        self.session = session
        self.playback = playback
        self.onLogout = onLogout
        _media = StateObject(wrappedValue: MediaViewModel(client: client))
        _playlists = StateObject(wrappedValue: PlaylistViewModel(client: client))
        _admin = StateObject(wrappedValue: AdminViewModel(currentUsername: session.user, client: client))
        _access = StateObject(wrappedValue: AccessControlViewModel(client: client))
        _lyrics = StateObject(wrappedValue: LyricsViewModel(client: client))
    }

    var body: some View {
        Group {
            if isNowPlayingPresented {
                NowPlayingView(playback: playback, lyrics: lyrics) {
                    isNowPlayingPresented = false
                }
                .onAppear { dismissFocusForEmptyQueue(playback.queueEntries.count) }
                .onChange(of: playback.queueEntries.count) { _, queueCount in
                    dismissFocusForEmptyQueue(queueCount)
                }
            } else if session.admin {
                adminLibraryWorkspace
            } else {
                libraryWorkspace
            }
        }
        .task {
            async let playlistLoad: Void = playlists.loadTree()
            if AccessManagementPolicy.allowsEntry(isAdmin: session.admin) {
                async let accessLoad: Void = access.load()
                _ = await (playlistLoad, accessLoad)
            } else {
                _ = await playlistLoad
            }
        }
    }

    private var adminLibraryWorkspace: some View {
        libraryWorkspace
            .fileImporter(
                isPresented: $importing,
                allowedContentTypes: [.audio],
                allowsMultipleSelection: true
            ) { result in
                if case let .success(urls) = result {
                    Task { await workflow.importFiles(urls) }
                }
            }
            .modifier(
                LibraryImportDropModifier(enabled: true, isTargeted: $isDropTargeted) { urls in
                    Task { await workflow.importFiles(urls) }
                }
            )
            .sheet(isPresented: accessTargetIsPresented) {
                accessManagementSheet
            }
    }

    private var libraryWorkspace: some View {
        VStack(spacing: 0) {
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

            libraryBottomAccessory
        }
        .toolbar {
            if session.admin {
                ForEach(
                    Array(LibraryViewPolicy.managementActions(isAdmin: true).enumerated()),
                    id: \.offset
                ) { _, action in
                    managementButton(action)
                }
            }
            Menu {
                Button("退出登录", role: .destructive, action: onLogout)
            } label: {
                Label(session.user, systemImage: "person.crop.circle")
            }
        }
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
    }

    @ViewBuilder private var libraryBottomAccessory: some View {
        if session.admin, workflow.isDrawerPresented {
            TaskDrawerView(model: workflow)
                .frame(maxHeight: 300)
        }
        if PlaybackViewPolicy.showsPlayerBar(queueCount: playback.queueEntries.count) {
            PlayerBar(playback: playback) {
                isNowPlayingPresented = true
            }
        }
    }

    @ViewBuilder private var accessManagementSheet: some View {
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
                .id(access.rule(for: target).map(AccessRuleEditorIdentity.init))
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

    private func dismissFocusForEmptyQueue(_ queueCount: Int) {
        if PlaybackViewPolicy.shouldDismissFocus(queueCount: queueCount) {
            isNowPlayingPresented = false
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
                PlaylistDetailView(
                    detail: detail,
                    playlists: playlists,
                    playback: playback
                )
            } else if playlists.isLoadingDetail {
                ProgressView("正在加载歌单…")
            } else if let message = playlists.errorMessage {
                ContentUnavailableView {
                    Label("无法打开歌单", systemImage: "wifi.exclamationmark")
                } description: {
                    Text(message)
                } actions: {
                    Button("重试") {
                        Task { await playlists.openPlaylist(id: id) }
                    }
                    .buttonStyle(.borderedProminent)
                }
                .padding(32)
            } else {
                ProgressView().task(id: id) { await playlists.openPlaylist(id: id) }
            }
        case .library, .none:
            if session.admin {
                LibraryBrowserView(
                    browse: browse,
                    search: search,
                    artistDetail: artistDetail,
                    client: client,
                    playback: playback,
                    playlists: playlists,
                    session: session,
                    onImportMusic: { importing = true },
                    onScanLibrary: { Task { await workflow.scanLibrary() } },
                    onShowTasks: { workflow.isDrawerPresented.toggle() },
                    onManageAccess: { accessTarget = $0 }
                )
            } else {
                LibraryBrowserView(
                    browse: browse,
                    search: search,
                    artistDetail: artistDetail,
                    client: client,
                    playback: playback,
                    playlists: playlists,
                    session: session
                )
            }
        }
    }

    @ViewBuilder private func managementButton(_ action: LibraryManagementAction) -> some View {
        switch action {
        case .importMusic:
            Button { importing = true } label: { Label("导入音乐", systemImage: "plus") }
        case .scanLibrary:
            Button { Task { await workflow.scanLibrary() } } label: {
                Label("扫描曲库", systemImage: "arrow.clockwise")
            }
        case .showTasks:
            Button { workflow.isDrawerPresented.toggle() } label: {
                Label("任务", systemImage: "tray.full")
            }
        }
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
}

private struct LibraryImportDropModifier: ViewModifier {
    let enabled: Bool
    @Binding var isTargeted: Bool
    let importFiles: ([URL]) -> Void

    @ViewBuilder
    func body(content: Content) -> some View {
        if enabled {
            content
                .dropDestination(for: URL.self) { urls, _ in
                    importFiles(urls)
                    return true
                } isTargeted: { isTargeted = $0 }
                .overlay {
                    if isTargeted {
                        RoundedRectangle(cornerRadius: 18)
                            .fill(.indigo.opacity(0.2))
                            .overlay {
                                Label("松开以导入音乐", systemImage: "square.and.arrow.down")
                                    .font(.title2.bold())
                            }
                    }
                }
        } else {
            content
        }
    }
}
