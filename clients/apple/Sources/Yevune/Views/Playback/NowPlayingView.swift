import SwiftUI

struct NowPlayingView: View {
    @ObservedObject var playback: PlaybackController
    let close: () -> Void
    @State private var draggedTime: Double?
    @FocusState private var backButtonFocused: Bool

    var body: some View {
        GeometryReader { geometry in
            VStack(spacing: 0) {
                header

                HStack(alignment: .center, spacing: 52) {
                    identity
                        .frame(width: coverColumnWidth(for: geometry.size))

                    lyrics
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                .padding(.horizontal, 48)
                .padding(.vertical, 26)

                transport
                    .padding(.horizontal, 48)
                    .padding(.bottom, 28)
            }
        }
        .background {
            LinearGradient(
                colors: [.accentColor.opacity(0.08), .clear, .clear],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        }
        .onAppear { backButtonFocused = true }
    }

    private var header: some View {
        HStack {
            Button(action: close) {
                Label("返回曲库", systemImage: "chevron.left")
            }
            .buttonStyle(.plain)
            .keyboardShortcut(.cancelAction)
            .focused($backButtonFocused)
            .accessibilityLabel("返回曲库，继续播放")

            Spacer()
        }
        .padding(.horizontal, 24)
        .padding(.top, 20)
    }

    private var identity: some View {
        VStack(alignment: .leading, spacing: 18) {
            AsyncImage(url: playback.coverURL) { image in
                image.resizable().scaledToFill()
            } placeholder: {
                ZStack {
                    Color.secondary.opacity(0.12)
                    Image(systemName: "music.note")
                        .font(.system(size: 52, weight: .light))
                        .foregroundStyle(.secondary)
                }
            }
            .aspectRatio(1, contentMode: .fit)
            .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
            .shadow(color: .black.opacity(0.16), radius: 24, y: 12)
            .accessibilityLabel("当前歌曲封面")

            VStack(alignment: .leading, spacing: 7) {
                Text(playback.currentTrack?.title ?? "未在播放")
                    .font(.title.bold())
                    .lineLimit(2)
                Text(playback.currentTrack?.artist ?? "未知艺人")
                    .font(.title3)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                if let album = playback.currentTrack?.album, !album.isEmpty {
                    Text(album)
                        .font(.subheadline)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }
            }
            .accessibilityElement(children: .combine)
        }
    }

    private var lyrics: some View {
        ScrollView {
            Text(LyricsState.unavailable.displayText)
                .font(.system(size: 30, weight: .medium, design: .rounded))
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, minHeight: 280, alignment: .center)
                .padding(40)
        }
        .scrollIndicators(.never)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .accessibilityLabel(LyricsState.unavailable.displayText)
    }

    private var transport: some View {
        VStack(spacing: 12) {
            HStack(spacing: 12) {
                Button {
                    playback.setShuffled(!playback.isShuffled)
                } label: {
                    Image(systemName: "shuffle")
                        .foregroundStyle(playback.isShuffled ? Color.accentColor : .secondary)
                }
                .accessibilityLabel(playback.isShuffled ? "关闭随机播放" : "开启随机播放")

                Spacer()

                Button {
                    Task { await playback.previous() }
                } label: {
                    Image(systemName: "backward.fill")
                }
                .accessibilityLabel("上一首")

                Button {
                    playback.togglePlayPause()
                } label: {
                    Image(systemName: transportPresentation.primaryAction == .pause ? "pause.fill" : "play.fill")
                        .font(.title2)
                        .frame(width: 28, height: 28)
                }
                .buttonStyle(.borderedProminent)
                .buttonBorderShape(.circle)
                .accessibilityLabel(transportPresentation.primaryActionAccessibilityLabel)

                Button {
                    Task { await playback.next() }
                } label: {
                    Image(systemName: "forward.fill")
                }
                .accessibilityLabel("下一首")

                Spacer()

                Button {
                    playback.cycleRepeatMode()
                } label: {
                    Image(systemName: playback.repeatMode == .one ? "repeat.1" : "repeat")
                        .foregroundStyle(playback.repeatMode == .off ? .secondary : Color.accentColor)
                }
                .accessibilityLabel(repeatLabel)
            }
            .buttonStyle(.plain)

            HStack(spacing: 10) {
                Text(playbackTime(draggedTime ?? playback.elapsed))
                    .frame(width: 42, alignment: .trailing)
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
                Text(playbackTime(playback.duration))
                    .frame(width: 42, alignment: .leading)

                Button {
                    playback.toggleMuted()
                } label: {
                    Image(systemName: playback.isMuted || playback.volume == 0 ? "speaker.slash.fill" : "speaker.wave.2.fill")
                }
                .buttonStyle(.plain)
                .accessibilityLabel(playback.isMuted ? "取消静音" : "静音")

                Slider(
                    value: Binding(
                        get: { Double(playback.volume) },
                        set: { playback.setVolume(Float($0)) }
                    ),
                    in: 0...1
                )
                .frame(width: 92)
                .accessibilityLabel("音量")
            }
            .font(.caption.monospacedDigit())
            .foregroundStyle(.secondary)
        }
        .frame(maxWidth: 720)
        .frame(maxWidth: .infinity)
    }

    private var transportPresentation: PlaybackViewPolicy.TransportPresentation {
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

    private var repeatLabel: String {
        switch playback.repeatMode {
        case .off: "开启列表循环"
        case .all: "开启单曲循环"
        case .one: "关闭循环"
        }
    }

    private func coverColumnWidth(for size: CGSize) -> CGFloat {
        min(max(size.width * 0.34, 240), min(380, size.height * 0.58))
    }

    private func finishSeeking(_ isEditing: Bool) {
        guard !isEditing else { return }
        defer { draggedTime = nil }
        guard canSeek, let draggedTime else { return }
        playback.seek(to: draggedTime)
    }

    private func playbackTime(_ seconds: TimeInterval) -> String {
        guard seconds.isFinite, seconds > 0 else { return "0:00" }
        let total = Int(seconds.rounded(.down))
        return String(format: "%d:%02d", total / 60, total % 60)
    }
}
