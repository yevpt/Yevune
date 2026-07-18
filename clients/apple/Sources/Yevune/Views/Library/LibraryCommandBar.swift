import SwiftUI
import YevuneCoreFFI

enum LibraryCollectionStyle: String, CaseIterable, Identifiable {
    case grid = "网格"
    case list = "列表"

    var id: Self { self }
}

struct LibraryPresentation: Equatable {
    let layout: LibraryLayout
    let commandItems: [LibraryCommandItem]
    let managementActions: [LibraryManagementAction]
    let acceptsFileDrops: Bool

    init(width: CGFloat, isAdmin: Bool) {
        layout = LibraryViewPolicy.layout(for: width)
        commandItems = LibraryViewPolicy.commandBarItems(compact: layout == .compact)
        managementActions = layout == .regular
            ? LibraryViewPolicy.managementActions(isAdmin: isAdmin)
            : []
        acceptsFileDrops = LibraryViewPolicy.acceptsFileDrops(isAdmin: isAdmin)
    }

    static func emptyLibraryMessage(isAdmin: Bool) -> String {
        isAdmin ? "导入音乐" : "曲库尚无音乐，请联系管理员添加"
    }
}

struct LibrarySearchEmptyPresentation: Equatable {
    let message: String
    let clearActionTitle = "清除搜索"

    init(query: String) {
        message = "没有找到与“\(query)”匹配的音乐"
    }
}

enum LibraryNavigationSelection: Hashable {
    case artist(String)
    case album(String)
}

enum LibraryEscapeOutcome: Equatable {
    case clearSearch
    case closeNavigation
    case ignored
}

struct LibraryNavigationState: Equatable {
    var path: [LibraryNavigationSelection] = []
    private(set) var highlightedAlbumID: String?
    private(set) var highlightedArtistID: String?
    private(set) var routedAlbumSnapshot: Album?
    private var preservesPathDuringIdleAfterEscape = false

    init(path: [LibraryNavigationSelection] = []) {
        self.path = path
        if case .album(let id)? = path.last { highlightedAlbumID = id }
        if case .artist(let id)? = path.first { highlightedArtistID = id }
    }

    mutating func highlightAlbum(id: String) {
        highlightedAlbumID = id
        highlightedArtistID = nil
    }

    mutating func highlightArtist(id: String) {
        highlightedArtistID = id
        highlightedAlbumID = nil
    }

    mutating func openArtist(id: String) {
        highlightArtist(id: id)
        routedAlbumSnapshot = nil
        path = [.artist(id)]
    }

    mutating func openAlbum(_ album: Album) {
        highlightedAlbumID = album.id
        routedAlbumSnapshot = album
        if case .artist? = path.first {
            path = [path[0], .album(album.id)]
        } else {
            highlightedArtistID = nil
            path = [.album(album.id)]
        }
    }

    mutating func returnToLibrary() {
        path = []
        highlightedAlbumID = nil
        highlightedArtistID = nil
        routedAlbumSnapshot = nil
        preservesPathDuringIdleAfterEscape = false
    }

    mutating func setPath(_ value: [LibraryNavigationSelection]) {
        path = value
        guard let destination = value.last else {
            highlightedAlbumID = nil
            highlightedArtistID = nil
            routedAlbumSnapshot = nil
            preservesPathDuringIdleAfterEscape = false
            return
        }
        switch destination {
        case .artist(let id):
            highlightedArtistID = id
            highlightedAlbumID = nil
            routedAlbumSnapshot = nil
        case .album(let id):
            highlightedAlbumID = id
            if routedAlbumSnapshot?.id != id { routedAlbumSnapshot = nil }
            if case .artist(let artistID)? = value.first {
                highlightedArtistID = artistID
            } else {
                highlightedArtistID = nil
            }
        }
    }

    mutating func handleEscape(isSearchActive: Bool) -> LibraryEscapeOutcome {
        if isSearchActive {
            preservesPathDuringIdleAfterEscape = true
            return .clearSearch
        }
        guard !path.isEmpty else { return .ignored }
        returnToLibrary()
        return .closeNavigation
    }

    mutating func reconcile(visibleAlbumIDs: Set<String>, visibleArtistIDs: Set<String>) {
        if let root = path.first {
            let remainsVisible: Bool
            switch root {
            case .album(let id): remainsVisible = visibleAlbumIDs.contains(id)
            case .artist(let id): remainsVisible = visibleArtistIDs.contains(id)
            }
            if !remainsVisible {
                returnToLibrary()
                return
            }
        }

        let routedAlbumID = path.compactMap { selection -> String? in
            if case .album(let id) = selection { return id }
            return nil
        }.last
        let routedArtistID = path.compactMap { selection -> String? in
            if case .artist(let id) = selection { return id }
            return nil
        }.last
        if let highlightedAlbumID,
           highlightedAlbumID != routedAlbumID,
           !visibleAlbumIDs.contains(highlightedAlbumID) {
            self.highlightedAlbumID = nil
        }
        if let highlightedArtistID,
           highlightedArtistID != routedArtistID,
           !visibleArtistIDs.contains(highlightedArtistID) {
            self.highlightedArtistID = nil
        }
    }

    mutating func reconcileSearch(
        phase: LibrarySearchPhase,
        searchAlbumIDs: Set<String>,
        searchArtistIDs: Set<String>,
        browseAlbumIDs: Set<String>,
        browseArtistIDs: Set<String>
    ) {
        switch phase {
        case .debouncing, .loading:
            preservesPathDuringIdleAfterEscape = false
        case .results:
            preservesPathDuringIdleAfterEscape = false
            reconcile(visibleAlbumIDs: searchAlbumIDs, visibleArtistIDs: searchArtistIDs)
        case .empty, .failed:
            preservesPathDuringIdleAfterEscape = false
            reconcile(visibleAlbumIDs: [], visibleArtistIDs: [])
        case .idle:
            if !preservesPathDuringIdleAfterEscape {
                reconcile(visibleAlbumIDs: browseAlbumIDs, visibleArtistIDs: browseArtistIDs)
            }
        }
    }

    mutating func reconcileBrowse(visibleAlbumIDs: Set<String>, visibleArtistIDs: Set<String>) {
        guard !preservesPathDuringIdleAfterEscape else { return }
        reconcile(visibleAlbumIDs: visibleAlbumIDs, visibleArtistIDs: visibleArtistIDs)
    }

    mutating func resumeBrowseReconciliation() {
        preservesPathDuringIdleAfterEscape = false
    }
}

enum LibraryBrowsePresentation: Equatable {
    case loading
    case initialFailure(String)
    case content(isRefreshing: Bool, refreshError: String?)
    case empty(String)

    static func resolve(
        contentCount: Int,
        isRefreshing: Bool,
        initialError: String?,
        refreshError: String?,
        isAdmin: Bool
    ) -> Self {
        if contentCount > 0 {
            return .content(isRefreshing: isRefreshing, refreshError: refreshError)
        }
        if let initialError {
            return .initialFailure(initialError)
        }
        if isRefreshing {
            return .loading
        }
        return .empty(LibraryPresentation.emptyLibraryMessage(isAdmin: isAdmin))
    }
}

struct LibraryCommandBar: View {
    @ObservedObject var browse: LibraryBrowseViewModel
    @ObservedObject var search: LibrarySearchViewModel
    let presentation: LibraryPresentation
    @Binding var collectionStyle: LibraryCollectionStyle
    let onImportMusic: () -> Void
    let onScanLibrary: () -> Void
    let onShowTasks: () -> Void

    @FocusState private var searchFocused: Bool
    @State private var filterPresented = false
    @State private var fromYear: UInt32 = 2000
    @State private var toYear: UInt32 = UInt32(Calendar.current.component(.year, from: Date()))

    var body: some View {
        HStack(spacing: 12) {
            Picker("分区", selection: sectionBinding) {
                Text("专辑").tag(LibrarySection.albums)
                Text("艺人").tag(LibrarySection.artists)
            }
            .pickerStyle(.segmented)
            .frame(width: 150)

            TextField("搜索艺人、专辑或曲目", text: searchBinding)
                .textFieldStyle(.roundedBorder)
                .focused($searchFocused)
                .frame(minWidth: 220, maxWidth: 360)
                .background {
                    Button("聚焦搜索") { searchFocused = true }
                        .keyboardShortcut("f", modifiers: .command)
                        .opacity(0)
                        .accessibilityHidden(true)
                }

            Spacer(minLength: 8)

            if presentation.commandItems.contains(.summary) {
                Text(summary)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Button {
                filterPresented.toggle()
            } label: {
                Label("筛选", systemImage: "line.3.horizontal.decrease.circle")
            }
            .popover(isPresented: $filterPresented) {
                filterControls
                    .padding()
                    .frame(width: 300)
            }

            if presentation.commandItems.contains(.viewStyle) {
                stylePicker
            }

            managementMenu
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    private var sectionBinding: Binding<LibrarySection> {
        Binding(get: { browse.section }, set: browse.selectSection)
    }

    private var searchBinding: Binding<String> {
        Binding(get: { search.input }, set: search.setInput)
    }

    private var summary: String {
        browse.section == .albums ? "\(browse.albums.count) 张专辑" : "\(browse.artists.count) 位艺人"
    }

    private var stylePicker: some View {
        Picker("视图", selection: $collectionStyle) {
            ForEach(LibraryCollectionStyle.allCases) { style in
                Text(style.rawValue).tag(style)
            }
        }
        .pickerStyle(.segmented)
        .frame(width: 120)
    }

    @ViewBuilder private var managementMenu: some View {
        if !presentation.managementActions.isEmpty {
            Menu {
                if presentation.managementActions.contains(.importMusic) {
                    Button("导入音乐", action: onImportMusic)
                }
                if presentation.managementActions.contains(.scanLibrary) {
                    Button("扫描曲库", action: onScanLibrary)
                }
                if presentation.managementActions.contains(.showTasks) {
                    Button("显示任务", action: onShowTasks)
                }
            } label: {
                Label("管理曲库", systemImage: "ellipsis.circle")
            }
        }
    }

    private var filterControls: some View {
        VStack(alignment: .leading, spacing: 14) {
            Picker("排序", selection: sortBinding) {
                Text("最近入库").tag(AlbumSort.newest)
                Text("专辑名称").tag(AlbumSort.alphabeticalByName)
                Text("艺人名称").tag(AlbumSort.alphabeticalByArtist)
                Text("最常播放").tag(AlbumSort.frequent)
                Text("最近播放").tag(AlbumSort.recent)
                Text("我的收藏").tag(AlbumSort.starred)
            }

            Picker("流派", selection: genreBinding) {
                Text("全部流派").tag(String?.none)
                ForEach(browse.genres, id: \.value) { genre in
                    Text(genre.value).tag(String?.some(genre.value))
                }
            }

            Stepper("起始年份：\(fromYear)", value: $fromYear, in: 1900...2100)
            Stepper("结束年份：\(toYear)", value: $toYear, in: 1900...2100)
            Button("应用年份") {
                browse.selectCriterion(.yearRange(from: fromYear, to: toYear))
            }
            if presentation.layout == .compact {
                Divider()
                stylePicker
            }
            if let message = browse.validationMessage {
                Text(message).font(.caption).foregroundStyle(.red)
            }
        }
    }

    private var sortBinding: Binding<AlbumSort> {
        Binding(
            get: {
                if case .sort(let value) = browse.albumCriterion { return value }
                return .newest
            },
            set: { browse.selectCriterion(.sort($0)) }
        )
    }

    private var genreBinding: Binding<String?> {
        Binding(
            get: {
                if case .genre(let value) = browse.albumCriterion { return value }
                return nil
            },
            set: { value in
                browse.selectCriterion(value.map(AlbumBrowseCriterion.genre) ?? .sort(.newest))
            }
        )
    }
}
