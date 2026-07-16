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

    func testDetailBackActionOnlyClearsNavigationSelection() {
        var navigation: LibraryNavigationSelection? = .artist("artist-1")

        LibraryNavigationAction.returnToLibrary.apply(to: &navigation)

        XCTAssertNil(navigation)
    }
}
