import XCTest
@testable import Yevune

final class LibraryPresentationTests: XCTestCase {
    func testCompactPresentationFitsMinimumWindow() {
        let presentation = LibraryPresentation(width: 920, isAdmin: false)

        XCTAssertEqual(presentation.layout, .compact)
        XCTAssertEqual(presentation.commandItems, [.section, .search, .filter])
        XCTAssertEqual(presentation.managementActions, [])
    }

    func testRegularPresentationUsesInspector() {
        XCTAssertEqual(LibraryPresentation(width: 1_280, isAdmin: true).layout, .regular)
    }

    func testCompactPresentationNeverConstructsManagementMenu() {
        XCTAssertEqual(LibraryPresentation(width: 920, isAdmin: true).managementActions, [])
        XCTAssertEqual(
            LibraryPresentation(width: 1_280, isAdmin: true).managementActions,
            [.importMusic, .scanLibrary, .showTasks]
        )
    }

    func testEmptyLibraryGuidesAdministratorsToImportMusic() {
        XCTAssertEqual(LibraryPresentation.emptyLibraryMessage(isAdmin: true), "导入音乐")
    }

    func testEmptyLibraryTellsMembersToContactAdministrator() {
        XCTAssertEqual(
            LibraryPresentation.emptyLibraryMessage(isAdmin: false),
            "曲库尚无音乐，请联系管理员添加"
        )
    }

    func testSearchEmptyStateIncludesActualQueryAndClearAction() {
        let state = LibrarySearchEmptyPresentation(query: "坂本龍一")

        XCTAssertTrue(state.message.contains("坂本龍一"))
        XCTAssertEqual(state.clearActionTitle, "清除搜索")
    }

    func testEscapeClearsSearchBeforeClosingNavigationWithoutPlaybackAction() {
        var navigation = LibraryNavigationState(path: [.artist("artist-1")])

        XCTAssertEqual(navigation.handleEscape(isSearchActive: true), .clearSearch)
        navigation.reconcileSearch(
            phase: .idle,
            searchAlbumIDs: [],
            searchArtistIDs: [],
            browseAlbumIDs: [],
            browseArtistIDs: []
        )
        navigation.reconcileSearch(
            phase: .idle,
            searchAlbumIDs: [],
            searchArtistIDs: [],
            browseAlbumIDs: [],
            browseArtistIDs: []
        )
        XCTAssertEqual(navigation.path, [.artist("artist-1")])
        XCTAssertEqual(navigation.handleEscape(isSearchActive: false), .closeNavigation)
        XCTAssertTrue(navigation.path.isEmpty)
    }

    func testCompactNavigationPreservesArtistWhenOpeningAlbumAndBackReturnsToArtist() {
        var navigation = LibraryNavigationState()

        navigation.openArtist(id: "artist-1")
        navigation.openAlbum(id: "album-1")
        XCTAssertEqual(navigation.path, [.artist("artist-1"), .album("album-1")])

        navigation.setPath([.artist("artist-1")])
        XCTAssertEqual(navigation.path, [.artist("artist-1")])
        XCTAssertEqual(navigation.highlightedArtistID, "artist-1")
        XCTAssertNil(navigation.highlightedAlbumID)
    }

    func testAlbumSingleClickHighlightsAndOpenAdvancesNavigation() {
        var navigation = LibraryNavigationState()

        navigation.highlightAlbum(id: "album-1")
        XCTAssertEqual(navigation.highlightedAlbumID, "album-1")
        XCTAssertTrue(navigation.path.isEmpty)

        navigation.openAlbum(id: "album-1")
        XCTAssertEqual(navigation.path, [.album("album-1")])
        navigation.returnToLibrary()
        XCTAssertNil(navigation.highlightedAlbumID)
    }

    func testArtistSingleClickHighlightsAndOpenAdvancesNavigation() {
        var navigation = LibraryNavigationState()

        navigation.highlightArtist(id: "artist-1")
        XCTAssertEqual(navigation.highlightedArtistID, "artist-1")
        XCTAssertTrue(navigation.path.isEmpty)

        navigation.openArtist(id: "artist-1")
        XCTAssertEqual(navigation.path, [.artist("artist-1")])
    }

    func testPendingSearchKeepsCurrentRouteUntilResultsAreDetermined() {
        var navigation = LibraryNavigationState(path: [.album("album-1")])

        for phase in [LibrarySearchPhase.debouncing, .loading] {
            navigation.reconcileSearch(
                phase: phase,
                searchAlbumIDs: [],
                searchArtistIDs: [],
                browseAlbumIDs: ["album-1"],
                browseArtistIDs: []
            )
            XCTAssertEqual(navigation.path, [.album("album-1")])
        }
    }

    func testDeterminedSearchKeepsMatchingRouteAndClosesMissingRoute() {
        var navigation = LibraryNavigationState(path: [.album("album-1")])

        navigation.reconcileSearch(
            phase: .results,
            searchAlbumIDs: ["album-1"],
            searchArtistIDs: [],
            browseAlbumIDs: [],
            browseArtistIDs: []
        )
        XCTAssertEqual(navigation.path, [.album("album-1")])

        navigation.reconcileSearch(
            phase: .results,
            searchAlbumIDs: ["album-2"],
            searchArtistIDs: [],
            browseAlbumIDs: [],
            browseArtistIDs: []
        )
        XCTAssertTrue(navigation.path.isEmpty)
    }

    func testEmptyAndFailedSearchCloseRoutesWhileIdleUsesBrowseVisibility() {
        for phase in [LibrarySearchPhase.empty, .failed("offline")] {
            var navigation = LibraryNavigationState(path: [.artist("artist-1")])
            navigation.reconcileSearch(
                phase: phase,
                searchAlbumIDs: [],
                searchArtistIDs: [],
                browseAlbumIDs: [],
                browseArtistIDs: ["artist-1"]
            )
            XCTAssertTrue(navigation.path.isEmpty)
        }

        var navigation = LibraryNavigationState(path: [.artist("artist-1")])
        navigation.reconcileSearch(
            phase: .idle,
            searchAlbumIDs: [],
            searchArtistIDs: [],
            browseAlbumIDs: [],
            browseArtistIDs: ["artist-1"]
        )
        XCTAssertEqual(navigation.path, [.artist("artist-1")])
    }

    func testRouteConsistencyClosesRootSelectionMissingFromVisibleCollection() {
        var navigation = LibraryNavigationState(path: [.album("old-album")])

        navigation.reconcile(visibleAlbumIDs: ["new-album"], visibleArtistIDs: [])

        XCTAssertTrue(navigation.path.isEmpty)
    }

    func testRouteConsistencyKeepsArtistAndNestedAlbumWhileArtistRemainsVisible() {
        var navigation = LibraryNavigationState(path: [.artist("artist-1"), .album("album-1")])

        navigation.reconcile(visibleAlbumIDs: [], visibleArtistIDs: ["artist-1"])

        XCTAssertEqual(navigation.path, [.artist("artist-1"), .album("album-1")])
        XCTAssertEqual(navigation.highlightedArtistID, "artist-1")
        XCTAssertEqual(navigation.highlightedAlbumID, "album-1")
    }

    func testInitialBrowseLoadingAndFailureDoNotPresentEmptyLibrary() {
        XCTAssertEqual(
            LibraryBrowsePresentation.resolve(
                contentCount: 0,
                isRefreshing: true,
                initialError: nil,
                refreshError: nil,
                isAdmin: false
            ),
            .loading
        )
        XCTAssertEqual(
            LibraryBrowsePresentation.resolve(
                contentCount: 0,
                isRefreshing: false,
                initialError: "offline",
                refreshError: nil,
                isAdmin: false
            ),
            .initialFailure("offline")
        )
    }

    func testExistingBrowseContentKeepsNonBlockingRefreshStatus() {
        XCTAssertEqual(
            LibraryBrowsePresentation.resolve(
                contentCount: 2,
                isRefreshing: false,
                initialError: nil,
                refreshError: "offline",
                isAdmin: true
            ),
            .content(isRefreshing: false, refreshError: "offline")
        )
    }

    func testEmptyBrowsePresentationUsesRoleSpecificMessageForEitherSection() {
        let member = LibraryBrowsePresentation.resolve(
            contentCount: 0,
            isRefreshing: false,
            initialError: nil,
            refreshError: nil,
            isAdmin: false
        )

        XCTAssertEqual(member, .empty("曲库尚无音乐，请联系管理员添加"))
    }
}
