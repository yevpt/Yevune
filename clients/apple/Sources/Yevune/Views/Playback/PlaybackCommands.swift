import SwiftUI

enum PlaybackCommandAction: Equatable {
    case togglePlayPause
    case previous
    case next
    case toggleShuffle
    case cycleRepeat
    case showQueue
    case showMiniPlayer
}

enum PlaybackCommandPolicy {
    static func isEnabled(_ action: PlaybackCommandAction, queueCount: Int) -> Bool {
        switch action {
        case .showQueue, .showMiniPlayer:
            true
        case .togglePlayPause, .previous, .next, .toggleShuffle, .cycleRepeat:
            queueCount > 0
        }
    }

    static func playPauseTitle(engineState: PlaybackEngineState) -> String {
        switch engineState {
        case .playing, .buffering:
            "暂停"
        case .idle, .paused:
            "播放"
        }
    }

    static func shuffleTitle(isShuffled: Bool) -> String {
        isShuffled ? "关闭随机播放" : "开启随机播放"
    }

    static func repeatTitle(mode: PlaybackRepeatMode) -> String {
        switch mode {
        case .off:
            "循环模式：关闭"
        case .all:
            "循环模式：列表"
        case .one:
            "循环模式：单曲"
        }
    }
}

enum PlaybackWindowID {
    static let queue = "playback-queue"
    static let miniPlayer = "mini-player"
}

struct PlaybackCommands: Commands {
    @ObservedObject var playback: PlaybackController
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandMenu("播放") {
            Button(PlaybackCommandPolicy.playPauseTitle(engineState: playback.engineState)) {
                playback.togglePlayPause()
            }
            .keyboardShortcut("p", modifiers: [.command, .option])
            .disabled(!isEnabled(.togglePlayPause))

            Button("上一首") {
                Task { await playback.previous() }
            }
            .keyboardShortcut(.leftArrow, modifiers: [.command, .option])
            .disabled(!isEnabled(.previous))

            Button("下一首") {
                Task { await playback.next() }
            }
            .keyboardShortcut(.rightArrow, modifiers: [.command, .option])
            .disabled(!isEnabled(.next))

            Divider()

            Button(PlaybackCommandPolicy.shuffleTitle(isShuffled: playback.isShuffled)) {
                playback.setShuffled(!playback.isShuffled)
            }
            .disabled(!isEnabled(.toggleShuffle))

            Button(PlaybackCommandPolicy.repeatTitle(mode: playback.repeatMode)) {
                playback.cycleRepeatMode()
            }
            .disabled(!isEnabled(.cycleRepeat))

            Divider()

            Button("显示播放队列") {
                openWindow(id: PlaybackWindowID.queue)
            }
            .keyboardShortcut("q", modifiers: [.command, .option])

            Button("显示迷你播放器") {
                openWindow(id: PlaybackWindowID.miniPlayer)
            }
            .keyboardShortcut("m", modifiers: [.command, .option])
        }
    }

    private func isEnabled(_ action: PlaybackCommandAction) -> Bool {
        PlaybackCommandPolicy.isEnabled(action, queueCount: playback.queueEntries.count)
    }
}
