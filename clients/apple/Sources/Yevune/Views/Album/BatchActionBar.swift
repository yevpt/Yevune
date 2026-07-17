import SwiftUI

struct BatchActionBar: View {
    let selectionCount: Int
    let isAdmin: Bool
    let isRunning: Bool
    let onPlay: () -> Void
    let onAddToPlaylist: () -> Void
    let onClearSelection: () -> Void
    let onEditTags: (() -> Void)?
    let onDelete: (() -> Void)?

    init(
        selectionCount: Int,
        isAdmin: Bool,
        isRunning: Bool,
        onPlay: @escaping () -> Void,
        onAddToPlaylist: @escaping () -> Void,
        onClearSelection: @escaping () -> Void,
        onEditTags: (() -> Void)? = nil,
        onDelete: (() -> Void)? = nil
    ) {
        self.selectionCount = selectionCount
        self.isAdmin = isAdmin
        self.isRunning = isRunning
        self.onPlay = onPlay
        self.onAddToPlaylist = onAddToPlaylist
        self.onClearSelection = onClearSelection
        self.onEditTags = onEditTags
        self.onDelete = onDelete
    }

    var body: some View {
        HStack(spacing: 12) {
            Text("已选择 \(selectionCount) 首")
                .font(.callout)
                .foregroundStyle(.secondary)

            Button(action: onPlay) {
                Label("播放", systemImage: "play.fill")
            }

            Button("加入歌单", action: onAddToPlaylist)
                .disabled(isRunning)

            if isAdmin, let onEditTags, let onDelete {
                Button("修改标签", action: onEditTags)
                    .disabled(isRunning)

                Menu("更多") {
                    Button("删除所选曲目", role: .destructive, action: onDelete)
                }
                .disabled(isRunning)
            }

            Spacer(minLength: 0)

            Button("取消选择", action: onClearSelection)
                .disabled(isRunning)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.bar)
        .fixedSize(horizontal: false, vertical: true)
        .accessibilityElement(children: .contain)
    }
}
