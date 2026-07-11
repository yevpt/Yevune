import SwiftUI

struct LibraryView: View {
    @ObservedObject var model: LibraryViewModel
    @State private var query = ""

    var body: some View {
        NavigationSplitView {
            List(model.albums, id: \.id) { album in
                VStack(alignment: .leading, spacing: 3) {
                    Text(album.name).font(.headline)
                    Text(album.artist ?? "未知艺人")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("曲库")
        } detail: {
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
        .task { await model.load() }
    }
}
