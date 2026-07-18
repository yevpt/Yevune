import SwiftUI

struct MediaFavoriteButton: View {
    @EnvironmentObject private var annotations: MediaAnnotationViewModel
    let target: MediaAnnotationTarget
    let starred: String?
    let rating: UInt8?
    var labeled = false

    private var snapshot: MediaAnnotationSnapshot {
        annotations.snapshot(
            for: target,
            fallbackStarred: starred != nil,
            fallbackRating: rating
        )
    }

    var body: some View {
        Button {
            Task {
                await annotations.setStarred(
                    target: target,
                    starred: !snapshot.isStarred
                )
            }
        } label: {
            if annotations.isMutating(target) {
                ProgressView().controlSize(.small)
            } else if labeled {
                Label(
                    snapshot.isStarred ? "已收藏" : "收藏",
                    systemImage: snapshot.isStarred ? "heart.fill" : "heart"
                )
            } else {
                Image(systemName: snapshot.isStarred ? "heart.fill" : "heart")
                    .foregroundStyle(snapshot.isStarred ? Color.accentColor : .secondary)
            }
        }
        .disabled(annotations.isMutating(target))
        .accessibilityLabel(snapshot.isStarred ? "取消收藏" : "收藏")
        .help(snapshot.isStarred ? "取消收藏" : "收藏")
        .task(id: seedIdentity) {
            annotations.seed(target: target, starred: starred, rating: rating)
        }
        .popover(isPresented: errorPresented, arrowEdge: .bottom) {
            annotationError
        }
    }

    private var seedIdentity: String {
        "\(target)-\(starred ?? "")-\(rating.map(String.init) ?? "")"
    }

    private var errorPresented: Binding<Bool> {
        Binding(
            get: { annotations.error(for: target) != nil },
            set: { if !$0 { annotations.clearError(for: target) } }
        )
    }

    private var annotationError: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("无法更新个人标注", systemImage: "exclamationmark.circle")
                .font(.headline)
            Text(annotations.error(for: target) ?? "请稍后重试")
                .foregroundStyle(.secondary)
            Button("关闭") { annotations.clearError(for: target) }
        }
        .padding(16)
        .frame(width: 280, alignment: .leading)
    }
}

struct MediaAnnotationMenuActions: View {
    @EnvironmentObject private var annotations: MediaAnnotationViewModel
    let target: MediaAnnotationTarget
    let starred: String?
    let rating: UInt8?
    var onStarredChanged: (Bool) -> Void = { _ in }

    private var snapshot: MediaAnnotationSnapshot {
        annotations.snapshot(
            for: target,
            fallbackStarred: starred != nil,
            fallbackRating: rating
        )
    }

    var body: some View {
        Button {
            Task {
                let starred = !snapshot.isStarred
                let succeeded = await annotations.setStarred(
                    target: target,
                    starred: starred
                )
                if succeeded { onStarredChanged(starred) }
            }
        } label: {
            Label(
                snapshot.isStarred ? "取消收藏" : "收藏",
                systemImage: snapshot.isStarred ? "heart.slash" : "heart"
            )
        }
        .disabled(annotations.isMutating(target))

        Menu {
            ForEach(1...5, id: \.self) { value in
                Button {
                    Task { await annotations.setRating(target: target, rating: UInt8(value)) }
                } label: {
                    Label(
                        "\(value) 星",
                        systemImage: snapshot.rating == UInt8(value) ? "checkmark" : "star"
                    )
                }
            }
            if snapshot.rating != nil {
                Divider()
                Button("清除评分") {
                    Task { await annotations.setRating(target: target, rating: nil) }
                }
            }
        } label: {
            Label(
                snapshot.rating.map { "\($0) 星" } ?? "评分",
                systemImage: snapshot.rating == nil ? "star" : "star.fill"
            )
        }
        .disabled(annotations.isMutating(target))
        .task(id: target) {
            annotations.seed(target: target, starred: starred, rating: rating)
        }
    }
}

struct MediaAnnotationIndicator: View {
    @EnvironmentObject private var annotations: MediaAnnotationViewModel
    @State private var errorPresented = false
    let target: MediaAnnotationTarget
    let starred: String?
    let rating: UInt8?

    private var snapshot: MediaAnnotationSnapshot {
        annotations.snapshot(
            for: target,
            fallbackStarred: starred != nil,
            fallbackRating: rating
        )
    }

    var body: some View {
        HStack(spacing: 4) {
            if snapshot.isStarred {
                Image(systemName: "heart.fill")
                    .foregroundStyle(Color.accentColor)
                    .accessibilityLabel("已收藏")
            }
            if let rating = snapshot.rating {
                Label("\(rating)", systemImage: "star.fill")
                    .accessibilityLabel("评分 \(rating) 星")
            }
            if annotations.error(for: target) != nil {
                Button {
                    errorPresented = true
                } label: {
                    Image(systemName: "exclamationmark.circle.fill")
                        .foregroundStyle(.red)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("个人标注更新失败")
                .popover(isPresented: $errorPresented, arrowEdge: .bottom) {
                    VStack(alignment: .leading, spacing: 10) {
                        Text("无法更新个人标注").font(.headline)
                        Text(annotations.error(for: target) ?? "请稍后重试")
                            .foregroundStyle(.secondary)
                        Button("关闭") {
                            annotations.clearError(for: target)
                            errorPresented = false
                        }
                    }
                    .padding(16)
                    .frame(width: 280, alignment: .leading)
                }
            }
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .task(id: target) {
            annotations.seed(target: target, starred: starred, rating: rating)
        }
    }
}
