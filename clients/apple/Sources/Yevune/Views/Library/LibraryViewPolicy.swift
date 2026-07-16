import Foundation
import YevuneCoreFFI

enum LibraryLayout: Equatable {
    case compact
    case regular
}

enum LibraryCommandItem: Equatable {
    case section
    case search
    case filter
    case summary
    case viewStyle
}

enum LibraryManagementAction: Equatable {
    case importMusic
    case scanLibrary
    case showTasks
}

enum LibraryViewPolicy {
    static func layout(for width: CGFloat) -> LibraryLayout {
        width >= 1_180 ? .regular : .compact
    }

    static func commandBarItems(compact: Bool) -> [LibraryCommandItem] {
        compact
            ? [.section, .search, .filter]
            : [.section, .search, .summary, .filter, .viewStyle]
    }

    static func managementActions(isAdmin: Bool) -> [LibraryManagementAction] {
        isAdmin ? [.importMusic, .scanLibrary, .showTasks] : []
    }

    static func acceptsFileDrops(isAdmin: Bool) -> Bool {
        isAdmin
    }

    static func artistSectionTitle(_ artist: Artist) -> String {
        let source = artist.sortName ?? artist.name
        guard let scalar = source
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .unicodeScalars
            .first,
            scalar.isASCII,
            CharacterSet.letters.contains(scalar)
        else { return "#" }

        return String(scalar).uppercased()
    }
}
