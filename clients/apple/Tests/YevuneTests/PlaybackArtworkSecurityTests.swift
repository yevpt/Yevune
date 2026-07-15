import Foundation
import XCTest

final class PlaybackArtworkSecurityTests: XCTestCase {
    func testEveryPlaybackSurfaceConsumesControllerDecodedArtwork() throws {
        for relativePath in [
            "Sources/Yevune/Views/Playback/PlayerBar.swift",
            "Sources/Yevune/Views/Playback/NowPlayingView.swift",
            "Sources/Yevune/Views/Playback/MiniPlayerView.swift",
        ] {
            let source = try String(contentsOf: packageRoot.appending(path: relativePath), encoding: .utf8)
            XCTAssertTrue(source.contains("DecodedArtworkView(image: playback.artwork)"), relativePath)
            XCTAssertFalse(source.contains("AsyncImage"), relativePath)
            XCTAssertFalse(source.contains("URLSession.shared"), relativePath)
        }
    }

    func testEveryAuthenticatedLibraryArtworkSurfaceAvoidsAsyncImage() throws {
        for relativePath in [
            "Sources/Yevune/Views/AlbumGridView.swift",
            "Sources/Yevune/Views/MediaDetailView.swift",
        ] {
            let source = try String(contentsOf: packageRoot.appending(path: relativePath), encoding: .utf8)
            XCTAssertTrue(source.contains("AuthenticatedArtworkView"), relativePath)
            XCTAssertFalse(source.contains("AsyncImage"), relativePath)
        }
    }

    private var packageRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
    }
}
