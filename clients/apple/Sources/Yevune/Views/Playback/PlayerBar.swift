import SwiftUI

struct PlayerBar: View {
    @ObservedObject var playback: PlaybackController
    let openNowPlaying: (() -> Void)?
    @State private var queuePresented = false

    var body: some View {
        HStack(spacing: 18) {
            CurrentTrackSummary(playback: playback, action: openNowPlaying)
                .frame(width: 240, alignment: .leading)

            Spacer(minLength: 0)

            TransportControls(playback: playback)
                .frame(minWidth: 300, idealWidth: 400, maxWidth: 480)

            Spacer(minLength: 0)

            PlaybackOptions(playback: playback, queuePresented: $queuePresented)
                .frame(width: 260, alignment: .trailing)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 9)
        .frame(minHeight: 76)
        .background(.regularMaterial)
        .overlay(alignment: .top) { Divider() }
        .popover(isPresented: $queuePresented, arrowEdge: .top) {
            QueuePanel(playback: playback)
        }
    }
}

private struct CurrentTrackSummary: View {
    @ObservedObject var playback: PlaybackController
    let action: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Group {
                if let action {
                    Button(action: action) {
                        summaryContent
                    }
                    .buttonStyle(.plain)
                    .help("打开当前播放")
                } else {
                    summaryContent
                }
            }
            .accessibilityLabel("当前播放：\(playback.currentTrack?.title ?? "无")")

            if let error = PlaybackViewPolicy.errorPresentation(for: playback.errorMessage) {
                Label(error.message, systemImage: "exclamationmark.circle.fill")
                    .font(.caption)
                    .foregroundStyle(.red)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityLabel(error.accessibilityLabel)
            }
        }
    }

    private var summaryContent: some View {
        HStack(spacing: 10) {
            AsyncImage(url: playback.coverURL) { image in
                image.resizable().scaledToFill()
            } placeholder: {
                ZStack {
                    Color.secondary.opacity(0.12)
                    Image(systemName: "music.note")
                        .foregroundStyle(.secondary)
                }
            }
            .frame(width: 52, height: 52)
            .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))

            VStack(alignment: .leading, spacing: 3) {
                Text(playback.currentTrack?.title ?? "未在播放")
                    .font(.headline)
                    .lineLimit(1)
                Text(playback.currentTrack?.artist ?? "未知艺人")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
    }
}

private struct TransportControls: View {
    @ObservedObject var playback: PlaybackController
    @State private var draggedTime: Double?

    var body: some View {
        VStack(spacing: 5) {
            HStack(spacing: 22) {
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
                        .font(.title3)
                        .frame(width: 22, height: 22)
                }
                .buttonStyle(.borderedProminent)
                .buttonBorderShape(.circle)
                .accessibilityLabel(transport.primaryActionAccessibilityLabel)

                Button {
                    Task { await playback.next() }
                } label: {
                    Image(systemName: "forward.fill")
                }
                .accessibilityLabel("下一首")
            }
            .buttonStyle(.plain)

            if transport.showsBufferingIndicator, let statusText = transport.statusText {
                HStack(spacing: 5) {
                    ProgressView()
                        .controlSize(.small)
                    Text(statusText)
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            }

            HStack(spacing: 8) {
                Text(playbackTime(draggedTime ?? playback.elapsed))
                    .frame(width: 38, alignment: .trailing)
                Slider(
                    value: Binding(
                        get: { draggedTime ?? min(playback.elapsed, sliderUpperBound) },
                        set: { draggedTime = $0 }
                    ),
                    in: 0...sliderUpperBound,
                    onEditingChanged: finishSeeking
                )
                .accessibilityLabel("播放进度")
                .accessibilityValue(progressAccessibilityValue)
                .disabled(!canSeek)
                Text(playbackTime(playback.duration))
                    .frame(width: 38, alignment: .leading)
            }
            .font(.caption.monospacedDigit())
            .foregroundStyle(.secondary)
        }
    }

    private var transport: PlaybackViewPolicy.TransportPresentation {
        PlaybackViewPolicy.transportPresentation(for: playback.engineState)
    }

    private var canSeek: Bool {
        PlaybackViewPolicy.canSeek(duration: playback.duration)
    }

    private var progressAccessibilityValue: String {
        PlaybackViewPolicy.progressAccessibilityValue(
            elapsed: draggedTime ?? playback.elapsed,
            duration: playback.duration
        ) ?? "总时长未知"
    }

    private var sliderUpperBound: Double {
        max(playback.duration, 1)
    }

    private func finishSeeking(_ isEditing: Bool) {
        guard !isEditing else { return }
        defer { self.draggedTime = nil }
        guard canSeek, let draggedTime else { return }
        playback.seek(to: draggedTime)
    }
}

private struct PlaybackOptions: View {
    @ObservedObject var playback: PlaybackController
    @Binding var queuePresented: Bool

    var body: some View {
        HStack(spacing: 12) {
            Button {
                playback.setShuffled(!playback.isShuffled)
            } label: {
                Image(systemName: "shuffle")
                    .foregroundStyle(playback.isShuffled ? Color.accentColor : .secondary)
            }
            .accessibilityLabel(playback.isShuffled ? "关闭随机播放" : "开启随机播放")

            Button {
                playback.cycleRepeatMode()
            } label: {
                Image(systemName: playback.repeatMode == .one ? "repeat.1" : "repeat")
                    .foregroundStyle(playback.repeatMode == .off ? .secondary : Color.accentColor)
            }
            .accessibilityLabel(repeatLabel)

            Button {
                playback.toggleMuted()
            } label: {
                Image(systemName: playback.isMuted || playback.volume == 0 ? "speaker.slash.fill" : "speaker.wave.2.fill")
            }
            .accessibilityLabel(playback.isMuted ? "取消静音" : "静音")

            Slider(
                value: Binding(
                    get: { Double(playback.volume) },
                    set: { playback.setVolume(Float($0)) }
                ),
                in: 0...1
            )
            .frame(width: 74)
            .accessibilityLabel("音量")

            Button {
                queuePresented.toggle()
            } label: {
                Image(systemName: "list.bullet")
            }
            .accessibilityLabel("播放队列")
            .help("播放队列")
        }
        .buttonStyle(.plain)
    }

    private var repeatLabel: String {
        switch playback.repeatMode {
        case .off: "开启列表循环"
        case .all: "开启单曲循环"
        case .one: "关闭循环"
        }
    }
}
