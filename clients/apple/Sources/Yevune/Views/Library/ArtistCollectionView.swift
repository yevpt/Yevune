import SwiftUI
import YevuneCoreFFI

struct ArtistCollectionView: View {
    let artists: [Artist]
    let highlightedArtistID: String?
    let client: any MusicClientProviding
    let isAdmin: Bool
    let onHighlight: (Artist) -> Void
    let onOpen: (Artist) -> Void

    private var sections: [(String, [Artist])] {
        Dictionary(grouping: artists, by: LibraryViewPolicy.artistSectionTitle)
            .map { ($0.key, $0.value) }
            .sorted { lhs, rhs in
                if lhs.0 == "#" { return false }
                if rhs.0 == "#" { return true }
                return lhs.0 < rhs.0
            }
    }

    var body: some View {
        if artists.isEmpty {
            ContentUnavailableView {
                Label(LibraryPresentation.emptyLibraryMessage(isAdmin: isAdmin), systemImage: "person.2")
            }
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0, pinnedViews: .sectionHeaders) {
                    ForEach(sections, id: \.0) { title, members in
                        Section {
                            ForEach(members, id: \.id) { artist in
                                ArtistCollectionRow(
                                    artist: artist,
                                    client: client,
                                    isHighlighted: highlightedArtistID == artist.id,
                                    onHighlight: onHighlight,
                                    onOpen: onOpen
                                )
                            }
                        } header: {
                            Text(title)
                                .font(.system(size: 44, weight: .bold, design: .rounded))
                                .foregroundStyle(.secondary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, 18)
                                .padding(.vertical, 6)
                                .background(.background)
                                .accessibilityAddTraits(.isHeader)
                        }
                    }
                }
            }
        }
    }
}

private struct ArtistCollectionRow: View {
    let artist: Artist
    let client: any MusicClientProviding
    let isHighlighted: Bool
    let onHighlight: (Artist) -> Void
    let onOpen: (Artist) -> Void
    @State private var coverURL: URL?

    var body: some View {
        HStack(spacing: 12) {
            AuthenticatedArtworkView(url: coverURL) {
                Circle().fill(.quaternary).overlay {
                    Text(String(artist.name.prefix(1)).uppercased())
                        .font(.headline)
                        .foregroundStyle(.secondary)
                }
            }
            .frame(width: 44, height: 44)
            .clipShape(Circle())

            VStack(alignment: .leading, spacing: 2) {
                Text(artist.name).font(.headline)
                Text("\(artist.albumCount) 张专辑")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Image(systemName: "chevron.right").foregroundStyle(.tertiary)
        }
        .contentShape(Rectangle())
        .padding(.horizontal, 18)
        .padding(.vertical, 7)
        .background(
            isHighlighted ? Color.accentColor.opacity(0.16) : .clear,
            in: RoundedRectangle(cornerRadius: 6)
        )
        .focusable()
        .onTapGesture(count: 2) { onOpen(artist) }
        .onTapGesture { onHighlight(artist) }
        .onKeyPress(.return) {
            onOpen(artist)
            return .handled
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("艺人 \(artist.name)，\(artist.albumCount) 张专辑")
        .accessibilityAction(named: "打开艺人") { onOpen(artist) }
        .task(id: artist.coverArt) {
            guard let id = artist.coverArt,
                  let value = try? await client.coverArtURL(id: id, size: 160)
            else { coverURL = nil; return }
            coverURL = URL(string: value)
        }
    }
}
