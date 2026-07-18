import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class LyricsViewModelTests: XCTestCase {
    func testPrefersSyncedLyricsAndTracksCurrentLineWithOffset() async {
        let client = ImmediateLyricsClient(lyrics: [
            lyrics(synced: false, lines: [(nil, "静态歌词")]),
            lyrics(offset: 200, synced: true, lines: [(1_000, "第一句"), (2_000, "第二句")]),
        ])
        let model = LyricsViewModel(client: client)

        await model.load(trackID: "track-1")
        model.update(elapsed: 1.3)
        XCTAssertEqual(model.state, .synced(lines: ["第一句", "第二句"], currentLine: 0))

        model.update(elapsed: 2.3)
        XCTAssertEqual(model.state, .synced(lines: ["第一句", "第二句"], currentLine: 1))
    }

    func testFallsBackToUnsyncedLyrics() async {
        let client = ImmediateLyricsClient(lyrics: [
            lyrics(synced: false, lines: [(nil, "第一句"), (nil, "第二句")]),
        ])
        let model = LyricsViewModel(client: client)

        await model.load(trackID: "track-1")

        XCTAssertEqual(model.state, .unsynced("第一句\n第二句"))
    }

    func testLateResponseCannotOverwriteNewTrack() async {
        let client = SuspendedLyricsClient()
        let model = LyricsViewModel(client: client)
        let first = Task { await model.load(trackID: "track-a") }
        await client.waitForCallCount(1)
        let second = Task { await model.load(trackID: "track-b") }
        await client.waitForCallCount(2)

        await client.resolveCall(1, with: [lyrics(synced: false, lines: [(nil, "B")])])
        await second.value
        await client.resolveCall(0, with: [lyrics(synced: false, lines: [(nil, "A")])])
        await first.value

        XCTAssertEqual(model.state, .unsynced("B"))
    }
}

private actor ImmediateLyricsClient: MusicClientProviding {
    let lyrics: [StructuredLyrics]

    init(lyrics: [StructuredLyrics]) {
        self.lyrics = lyrics
    }

    func getLyricsBySongID(_ id: String) async throws -> [StructuredLyrics] { lyrics }
    func login(server: String, user: String, password: String) async throws -> SessionValue { throw LyricsTestError.failed }
    func logout() async {}
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { throw LyricsTestError.failed }
    func search(query: String) async throws -> SearchResult { throw LyricsTestError.failed }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track { throw LyricsTestError.failed }
    func updateTags(id: String, update: TagUpdate) async throws { throw LyricsTestError.failed }
    func deleteTrack(id: String) async throws { throw LyricsTestError.failed }
    func moveTrack(id: String, key: String) async throws { throw LyricsTestError.failed }
    func startScan() async throws -> ScanStatus { throw LyricsTestError.failed }
    func scanStatus() async throws -> ScanStatus { throw LyricsTestError.failed }
}

private actor SuspendedLyricsClient: MusicClientProviding {
    private var calls: [String] = []
    private var continuations: [CheckedContinuation<[StructuredLyrics], Error>] = []
    private var waiters: [(Int, CheckedContinuation<Void, Never>)] = []

    func getLyricsBySongID(_ id: String) async throws -> [StructuredLyrics] {
        calls.append(id)
        let ready = waiters.filter { calls.count >= $0.0 }
        waiters.removeAll { calls.count >= $0.0 }
        ready.forEach { $0.1.resume() }
        return try await withCheckedThrowingContinuation { continuations.append($0) }
    }

    func waitForCallCount(_ count: Int) async {
        guard calls.count < count else { return }
        await withCheckedContinuation { waiters.append((count, $0)) }
    }

    func resolveCall(_ index: Int, with lyrics: [StructuredLyrics]) {
        continuations[index].resume(returning: lyrics)
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue { throw LyricsTestError.failed }
    func logout() async {}
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { throw LyricsTestError.failed }
    func search(query: String) async throws -> SearchResult { throw LyricsTestError.failed }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track { throw LyricsTestError.failed }
    func updateTags(id: String, update: TagUpdate) async throws { throw LyricsTestError.failed }
    func deleteTrack(id: String) async throws { throw LyricsTestError.failed }
    func moveTrack(id: String, key: String) async throws { throw LyricsTestError.failed }
    func startScan() async throws -> ScanStatus { throw LyricsTestError.failed }
    func scanStatus() async throws -> ScanStatus { throw LyricsTestError.failed }
}

private enum LyricsTestError: Error { case failed }

private func lyrics(
    offset: Int64 = 0,
    synced: Bool,
    lines: [(UInt64?, String)]
) -> StructuredLyrics {
    StructuredLyrics(
        displayArtist: nil,
        displayTitle: nil,
        lang: nil,
        offset: offset,
        synced: synced,
        lines: lines.map { LyricLine(start: $0.0, value: $0.1) }
    )
}
