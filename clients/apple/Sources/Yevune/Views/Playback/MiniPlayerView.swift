import SwiftUI

struct MiniPlayerView: View {
    @ObservedObject var playback: PlaybackController
    @State private var draggedTime: Double?

    var body: some View {
        HStack(spacing: 14) {
            AsyncImage(url: playback.coverURL) { image in
                image.resizable().scaledToFill()
            } placeholder: {
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
                    Text(playback.currentTrack?.title ?? "未在播放")
                        .font(.headline)
                        .lineLimit(1)
                    Text(playback.currentTrack?.artist ?? "未知艺人")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
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

                Slider(
                    value: Binding(
                        get: { draggedTime ?? min(playback.elapsed, sliderUpperBound) },
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
        PlaybackViewPolicy.canSeek(duration: playback.duration)
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

    private func finishSeeking(_ isEditing: Bool) {
        guard !isEditing else { return }
        defer { draggedTime = nil }
        guard canSeek, let draggedTime else { return }
        playback.seek(to: draggedTime)
    }
}
