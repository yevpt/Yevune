import SwiftUI

struct BatchOperationResultView: View {
    let results: [TrackBatchItemResult]
    let currentTrackID: String?
    let isRunning: Bool
    let onStop: () -> Void
    let onRetryFailed: () -> Void
    let onDone: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var processedCount: Int {
        results.count { result in
            if case .pending = result.state { return false }
            return true
        }
    }

    private var failedResults: [TrackBatchItemResult] {
        results.filter {
            if case .failed = $0.state { return true }
            return false
        }
    }

    private var skippedResults: [TrackBatchItemResult] {
        results.filter { $0.state == .skipped }
    }

    private var currentTrack: TrackBatchItemResult? {
        results.first { $0.id == currentTrackID }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 6) {
                ProgressView(
                    value: Double(processedCount),
                    total: Double(max(results.count, 1))
                )
                .animation(reduceMotion ? nil : .default, value: processedCount)
                Text("已处理 \(processedCount) / \(results.count) 首")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }

            if let currentTrack {
                Label("正在处理：\(currentTrack.track.title)", systemImage: "waveform")
                    .lineLimit(1)
            }

            if isRunning {
                Button("停止", role: .destructive, action: onStop)
            }

            if !failedResults.isEmpty || !skippedResults.isEmpty {
                Divider()
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 10) {
                        ForEach(failedResults) { result in
                            if case let .failed(message) = result.state {
                                Label {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(result.track.title)
                                        Text(message)
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                } icon: {
                                    Image(systemName: "exclamationmark.circle")
                                        .foregroundStyle(.red)
                                }
                            }
                        }

                        ForEach(skippedResults) { result in
                            Label("\(result.track.title)：已跳过", systemImage: "forward.end")
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .frame(maxHeight: 180)
            }

            if !isRunning, !results.isEmpty {
                HStack {
                    if !failedResults.isEmpty {
                        Button("重试失败项", action: onRetryFailed)
                    }
                    Spacer(minLength: 0)
                    Button("完成", action: onDone)
                        .keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding()
    }
}
