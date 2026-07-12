import SwiftUI
import CoreFFI

struct TaskDrawerView: View {
    @ObservedObject var model: LibraryWorkflowViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("任务").font(.headline)
                Spacer()
                Button { model.isDrawerPresented = false } label: { Image(systemName: "xmark") }.buttonStyle(.plain)
            }
            ForEach(model.imports) { item in
                HStack {
                    Image(systemName: icon(item.state)).foregroundStyle(color(item.state))
                    VStack(alignment: .leading) {
                        Text(item.url.lastPathComponent)
                        if item.state == .uploading { ProgressView(value: item.progress) }
                        if item.state == .succeeded { Text("上传成功 · \(item.track?.title ?? "等待索引")").font(.caption).foregroundStyle(.green) }
                        if let error = item.errorMessage { Text(error).font(.caption).foregroundStyle(.red) }
                    }
                }
            }
            if model.isScanning { ProgressView("正在扫描并刷新曲库…") }
            if let result = model.scanResult {
                Text("扫描完成：新增 \(result.added) · 更新 \(result.updated) · 删除 \(result.deleted) · 未变化 \(result.unchanged)").font(.headline)
                ForEach(Array(result.changes.enumerated()), id: \.offset) { _, change in
                    Label("\(action(change.action)) · \(change.track.title) · \(change.track.album ?? "未知专辑")", systemImage: "music.note")
                        .font(.caption)
                }
                if result.changesTruncated { Text("变更较多，仅显示前 500 项").font(.caption).foregroundStyle(.secondary) }
            }
            if let error = model.scanError {
                HStack { Text("文件已上传，但索引失败：\(error)").foregroundStyle(.red); Button("重试扫描") { Task { await model.scanLibrary() } } }
            }
        }
        .padding(14).background(.regularMaterial).overlay(alignment: .top) { Divider() }
    }

    private func icon(_ state: ImportTaskState) -> String { switch state { case .waiting: "clock"; case .uploading: "arrow.up.circle"; case .succeeded: "checkmark.circle.fill"; case .failed: "xmark.circle.fill" } }
    private func color(_ state: ImportTaskState) -> Color { switch state { case .succeeded: .green; case .failed: .red; default: .secondary } }
    private func action(_ action: ScanAction) -> String { switch action { case .added: "新增"; case .updated: "更新"; case .deleted: "删除" } }
}
