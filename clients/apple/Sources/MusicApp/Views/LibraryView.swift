import SwiftUI
import CoreFFI
import UniformTypeIdentifiers

struct LibraryView: View {
    @ObservedObject var model: LibraryViewModel
    @State private var query = ""
    @State private var selection: Album?
    @StateObject private var media: MediaViewModel
    @StateObject private var workflow: LibraryWorkflowViewModel
    @State private var importing = false
    @State private var isDropTargeted = false

    init(model: LibraryViewModel) {
        self.model = model
        _media = StateObject(wrappedValue: MediaViewModel(client: model.clientForViews))
        _workflow = StateObject(wrappedValue: LibraryWorkflowViewModel(client: model.clientForViews, library: model))
    }

    var body: some View {
        NavigationSplitView {
            List(model.albums, id: \.id, selection: $selection) { album in
                VStack(alignment: .leading, spacing: 3) {
                    HStack { Text(album.name).font(.headline); if workflow.newAlbumIDs.contains(album.id) { Text("新增").font(.caption2).padding(.horizontal, 5).background(.green.opacity(0.2), in: Capsule()) } }
                    Text(album.artist ?? "未知艺人")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("曲库")
        } detail: {
            if let selection { MediaDetailView(album: selection, model: media) } else {
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
            .padding()
            }
        }
        .task { await model.load() }
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
    }
}
