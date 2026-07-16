import XCTest
import YevuneCoreFFI
@testable import Yevune

final class LibraryViewPolicyTests: XCTestCase {
    func testLayoutBreaksAt1180Points() {
        XCTAssertEqual(LibraryViewPolicy.layout(for: 1_179), .compact)
        XCTAssertEqual(LibraryViewPolicy.layout(for: 1_180), .regular)
    }

    func testCompactCommandBarContainsOnlySectionSearchAndFilter() {
        XCTAssertEqual(
            LibraryViewPolicy.commandBarItems(compact: true),
            [.section, .search, .filter]
        )
    }

    func testRegularCommandBarAddsSummaryAndViewStyle() {
        XCTAssertEqual(
            LibraryViewPolicy.commandBarItems(compact: false),
            [.section, .search, .summary, .filter, .viewStyle]
        )
    }

    func testMembersConstructNoManagementActions() {
        XCTAssertEqual(LibraryViewPolicy.managementActions(isAdmin: false), [])
        XCTAssertEqual(
            LibraryViewPolicy.managementActions(isAdmin: true),
            [.importMusic, .scanLibrary, .showTasks]
        )
    }

    func testArtistSectionUsesUppercaseLatinSortNameInitial() {
        XCTAssertEqual(
            LibraryViewPolicy.artistSectionTitle(artist(name: "Displayed", sortName: "  beta")),
            "B"
        )
        XCTAssertEqual(
            LibraryViewPolicy.artistSectionTitle(artist(name: "alpha", sortName: nil)),
            "A"
        )
    }

    func testNonASCIIDigitsAndSymbolsUseHashArtistSection() {
        XCTAssertEqual(LibraryViewPolicy.artistSectionTitle(artist(name: "周杰伦")), "#")
        XCTAssertEqual(LibraryViewPolicy.artistSectionTitle(artist(name: "2Pac")), "#")
        XCTAssertEqual(LibraryViewPolicy.artistSectionTitle(artist(name: "!Artist")), "#")
    }
}

private func artist(name: String, sortName: String? = nil) -> Artist {
    Artist(
        id: name,
        name: name,
        sortName: sortName,
        coverArt: nil,
        musicBrainzId: nil,
        albumCount: 0
    )
}
