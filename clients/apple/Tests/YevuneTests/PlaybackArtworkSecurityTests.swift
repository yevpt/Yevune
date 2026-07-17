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
            "Sources/Yevune/Views/Library/AlbumCollectionView.swift",
            "Sources/Yevune/Views/Album/AlbumHeaderView.swift",
        ] {
            let source = try String(contentsOf: packageRoot.appending(path: relativePath), encoding: .utf8)
            XCTAssertTrue(source.contains("AuthenticatedArtworkView"), relativePath)
            XCTAssertFalse(source.contains("AsyncImage"), relativePath)
        }
    }

    func testAlbumHeaderPassesRevisionAndCompletionToAuthenticatedArtworkView() throws {
        let source = try String(
            contentsOf: packageRoot.appending(path: "Sources/Yevune/Views/Album/AlbumHeaderView.swift"),
            encoding: .utf8
        )

        XCTAssertTrue(source.contains("revision: coverRevision"))
        XCTAssertTrue(source.contains("onArtworkLoad"))
        XCTAssertFalse(source.contains(".id(coverRevision)"))
    }

    func testAuthenticatedArtworkViewKeepsCacheFreeLoaderAndAvoidsSensitiveLogging() throws {
        let source = try String(
            contentsOf: packageRoot.appending(path: "Sources/Yevune/Views/AuthenticatedArtworkView.swift"),
            encoding: .utf8
        )

        XCTAssertTrue(source.contains("URLSessionPlaybackArtworkLoader"))
        XCTAssertFalse(source.contains("AsyncImage"))
        XCTAssertFalse(source.contains("print("))
        XCTAssertFalse(source.contains("Logger"))
        XCTAssertFalse(source.contains("UserDefaults"))
    }

    func testArtworkRetryIsOnlySelectedForAnArtworkErrorWithAResolvedURL() throws {
        let source = try String(
            contentsOf: packageRoot.appending(path: "Sources/Yevune/Views/MediaDetailView.swift"),
            encoding: .utf8
        )

        XCTAssertTrue(source.contains("model.coverError != nil && model.coverURL != nil"))
    }

    private var packageRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
    }
}
