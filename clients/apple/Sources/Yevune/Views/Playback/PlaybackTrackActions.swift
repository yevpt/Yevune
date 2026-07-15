import SwiftUI
import YevuneCoreFFI

struct PlaybackTrackActions: View {
    let track: Track
    @ObservedObject var playback: PlaybackController

    var body: some View {
        Button {
            Task { await playback.playNow(track) }
        } label: {
            Label("立即播放", systemImage: "play.fill")
        }

        Button {
            playback.playNext(track)
        } label: {
            Label("下一首播放", systemImage: "text.line.first.and.arrowtriangle.forward")
        }

        Button {
            playback.addToQueue(track)
        } label: {
            Label("加入队列", systemImage: "text.badge.plus")
        }
    }
}
