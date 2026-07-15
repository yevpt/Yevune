import SwiftUI
import YevuneCoreFFI

struct SearchPlaybackResults: View {
    let result: SearchResult
    @ObservedObject var playback: PlaybackController
    let selectAlbum: (Album) -> Void

    var body: some View {
        List {
            if !result.albums.isEmpty {
                Section("专辑") {
                    ForEach(result.albums, id: \.id) { album in
                        Button {
                            selectAlbum(album)
                        } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(album.name)
                                Text(album.artist ?? "未知艺人")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }

            if !result.tracks.isEmpty {
                Section("曲目") {
                    ForEach(Array(result.tracks.enumerated()), id: \.offset) { index, track in
                        SearchTrackRow(track: track)
                            .contentShape(Rectangle())
                            .focusable()
                            .onTapGesture(count: 2) {
                                playSearchResult(startingAt: index)
                            }
                            .onKeyPress(.return) {
                                playSearchResult(startingAt: index)
                                return .handled
                            }
                            .contextMenu {
                                PlaybackTrackActions(track: track, playback: playback)
                            }
                            .accessibilityAction(named: "播放") {
                                playSearchResult(startingAt: index)
                            }
                    }
                }
            }
        }
        .overlay {
            if result.albums.isEmpty, result.tracks.isEmpty {
                ContentUnavailableView.search
            }
        }
    }

    private func playSearchResult(startingAt index: Int) {
        let start = result.tracks.indices.contains(index) ? index : 0
        Task { await playback.play(tracks: result.tracks, startingAt: start) }
    }
}

private struct SearchTrackRow: View {
    let track: Track

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "music.note")
                .foregroundStyle(.secondary)
                .frame(width: 18)
            VStack(alignment: .leading, spacing: 2) {
                Text(track.title)
                Text(track.artist ?? "未知艺人")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if track.duration > 0 {
                Text(playbackTime(track.duration))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 2)
    }
}

func playbackTime(_ seconds: UInt32) -> String {
    playbackTime(TimeInterval(seconds))
}

func playbackTime(_ seconds: TimeInterval) -> String {
    guard seconds.isFinite, seconds > 0 else { return "0:00" }
    let total = Int(seconds.rounded(.down))
    return String(format: "%d:%02d", total / 60, total % 60)
}
