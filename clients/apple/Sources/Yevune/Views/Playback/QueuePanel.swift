import SwiftUI

struct QueuePanel: View {
    @ObservedObject var playback: PlaybackController

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("播放队列")
                        .font(.headline)
                    Text("\(playback.queueEntries.count) 首曲目")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("清空待播", role: .destructive) {
                    playback.clearUpcoming()
                }
                .disabled(!hasUpcomingEntries)
            }
            .padding(14)

            Divider()

            List {
                ForEach(Array(playback.queueEntries.enumerated()), id: \.element.id) { index, entry in
                    QueueEntryRow(
                        entry: entry,
                        index: index,
                        isCurrent: playback.currentQueueEntryID == entry.id,
                        playback: playback
                    )
                }
                .onMove(perform: moveEntries)
            }
            .listStyle(.inset)
        }
        .frame(minWidth: 380, idealWidth: 420, minHeight: 340, idealHeight: 460)
    }

    private func moveEntries(from offsets: IndexSet, to destination: Int) {
        guard let source = offsets.first, offsets.count == 1, !playback.queueEntries.isEmpty else { return }
        let adjusted = destination > source ? destination - 1 : destination
        let target = min(max(adjusted, 0), playback.queueEntries.count - 1)
        playback.moveQueueEntry(from: source, to: target)
    }

    private var hasUpcomingEntries: Bool {
        PlaybackViewPolicy.hasUpcomingQueueEntries(
            queueEntryIDs: playback.queueEntries.map(\.id),
            currentID: playback.currentQueueEntryID
        )
    }
}

private struct QueueEntryRow: View {
    let entry: QueueEntry
    let index: Int
    let isCurrent: Bool
    @ObservedObject var playback: PlaybackController

    var body: some View {
        HStack(spacing: 10) {
            Button {
                Task { await playback.playQueueEntry(id: entry.id) }
            } label: {
                Image(systemName: isCurrent ? "speaker.wave.2.fill" : "play.fill")
                    .foregroundStyle(isCurrent ? Color.accentColor : .secondary)
                    .frame(width: 20)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(isCurrent ? "当前播放" : "播放 \(entry.track.title)")

            VStack(alignment: .leading, spacing: 2) {
                Text(entry.track.title)
                    .fontWeight(isCurrent ? .semibold : .regular)
                    .lineLimit(1)
                Text(entry.track.artist ?? "未知艺人")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer()

            Menu {
                Button("上移") {
                    playback.moveQueueEntry(from: index, to: index - 1)
                }
                .disabled(index == 0)
                Button("下移") {
                    playback.moveQueueEntry(from: index, to: index + 1)
                }
                .disabled(index == playback.queueEntries.count - 1)
                Divider()
                Button("从队列移除", role: .destructive) {
                    playback.removeFromQueue(id: entry.id)
                }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .accessibilityLabel("\(entry.track.title) 的队列操作")
        }
        .padding(.vertical, 3)
        .accessibilityElement(children: .contain)
    }
}
