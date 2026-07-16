import SwiftUI
import YevuneCoreFFI

struct PlaybackTrackActions: View {
    let track: Track
    private let onPlayNow: () -> Void
    private let onPlayNext: () -> Void
    private let onAddToQueue: () -> Void

    init(track: Track, playback: PlaybackController) {
        self.track = track
        onPlayNow = { Task { await playback.playNow(track) } }
        onPlayNext = { playback.playNext(track) }
        onAddToQueue = { playback.addToQueue(track) }
    }

    init(
        track: Track,
        onPlayNow: @escaping () -> Void,
        onPlayNext: @escaping () -> Void,
        onAddToQueue: @escaping () -> Void
    ) {
        self.track = track
        self.onPlayNow = onPlayNow
        self.onPlayNext = onPlayNext
        self.onAddToQueue = onAddToQueue
    }

    var body: some View {
        Button(action: onPlayNow) {
            Label("立即播放", systemImage: "play.fill")
        }

        Button(action: onPlayNext) {
            Label("下一首播放", systemImage: "text.line.first.and.arrowtriangle.forward")
        }

        Button(action: onAddToQueue) {
            Label("加入队列", systemImage: "text.badge.plus")
        }
    }
}
