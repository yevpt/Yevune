import SwiftUI

struct MiniPlayerView: View {
    @ObservedObject var playback: PlaybackController
    @State private var draggedTime: Double?

    var body: some View {
        HStack(spacing: 14) {
            DecodedArtworkView(image: playback.artwork) {
                ZStack {
                    Color.secondary.opacity(0.12)
                    Image(systemName: "music.note")
                        .foregroundStyle(.secondary)
                }
            }
            .frame(width: 88, height: 88)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .accessibilityLabel("当前歌曲封面")

            VStack(alignment: .leading, spacing: 8) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .font(.headline)
                        .lineLimit(1)
                    HStack(spacing: 5) {
                        if case .buffering = status {
                            ProgressView()
                                .controlSize(.mini)
                                .accessibilityHidden(true)
                        }
                        Text(detail)
                            .font(.subheadline)
                            .foregroundStyle(statusColor)
                            .lineLimit(1)
                    }
                }
                .accessibilityElement(children: .combine)

                HStack(spacing: 18) {
                    Button {
                        Task { await playback.previous() }
                    } label: {
                        Image(systemName: "backward.fill")
                    }
                    .accessibilityLabel("上一首")

                    Button {
                        playback.togglePlayPause()
                    } label: {
                        Image(systemName: transport.primaryAction == .pause ? "pause.fill" : "play.fill")
                    }
                    .accessibilityLabel(transport.primaryActionAccessibilityLabel)

                    Button {
                        Task { await playback.next() }
                    } label: {
                        Image(systemName: "forward.fill")
                    }
                    .accessibilityLabel("下一首")
                }
                .buttonStyle(.plain)
                .disabled(!transportEnabled)

                Slider(
                    value: Binding(
                        get: {
                            PlaybackViewPolicy.sliderValue(
                                elapsed: draggedTime ?? playback.elapsed,
                                duration: playback.duration
                            )
                        },
                        set: { draggedTime = $0 }
                    ),
                    in: 0...sliderUpperBound,
                    onEditingChanged: finishSeeking
                )
                .disabled(!canSeek)
                .accessibilityLabel("播放进度")
                .accessibilityValue(progressAccessibilityValue)
            }
        }
        .padding(14)
    }

    private var transport: PlaybackViewPolicy.TransportPresentation {
        PlaybackViewPolicy.transportPresentation(for: playback.engineState)
    }

    private var canSeek: Bool {
        transportEnabled && PlaybackViewPolicy.canSeek(duration: playback.duration)
    }

    private var transportEnabled: Bool {
        PlaybackViewPolicy.isTransportEnabled(queueCount: playback.queueEntries.count)
    }

    private var sliderUpperBound: Double {
        PlaybackViewPolicy.sliderUpperBound(duration: playback.duration)
    }

    private var progressAccessibilityValue: String {
        PlaybackViewPolicy.progressAccessibilityValue(
            elapsed: draggedTime ?? playback.elapsed,
            duration: playback.duration
        ) ?? "总时长未知"
    }

    private var status: PlaybackViewPolicy.MiniPlayerStatus {
        PlaybackViewPolicy.miniPlayerStatus(
            queueCount: playback.queueEntries.count,
            engineState: playback.engineState,
            errorMessage: playback.errorMessage
        )
    }

    private var title: String {
        if case .empty(let message) = status { return message }
        return playback.currentTrack?.title ?? "未在播放"
    }

    private var detail: String {
        switch status {
        case .ready:
            playback.currentTrack?.artist ?? "未知艺人"
        case .empty:
            "从曲库选择歌曲后即可播放"
        case .buffering(let message), .error(let message):
            message
        }
    }

    private var statusColor: Color {
        if case .error = status { return .red }
        return .secondary
    }

    private func finishSeeking(_ isEditing: Bool) {
        guard !isEditing else { return }
        defer { draggedTime = nil }
        guard canSeek, let draggedTime else { return }
        playback.seek(to: draggedTime)
    }
}
