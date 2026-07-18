import YevuneCoreFFI

extension Track {
    init(
        id: String, title: String, album: String?, albumId: String?, artist: String?, artistId: String?,
        track: UInt32?, discNumber: UInt32?, year: UInt32?, genre: String?, coverArt: String?,
        size: UInt64, contentType: String?, suffix: String?, duration: UInt32, bitRate: UInt32,
        created: String?, path: String?
    ) {
        self.init(
            id: id, title: title, album: album, albumId: albumId, artist: artist, artistId: artistId,
            track: track, discNumber: discNumber, year: year, genre: genre, coverArt: coverArt,
            size: size, contentType: contentType, suffix: suffix, duration: duration, bitRate: bitRate,
            created: created, path: path, starred: nil, userRating: nil
        )
    }
}

extension Album {
    init(
        id: String, name: String, artist: String?, artistId: String?, coverArt: String?,
        songCount: UInt32, duration: UInt32, year: UInt32?, genre: String?, created: String?
    ) {
        self.init(
            id: id, name: name, artist: artist, artistId: artistId, coverArt: coverArt,
            songCount: songCount, duration: duration, year: year, genre: genre, created: created,
            starred: nil, userRating: nil
        )
    }
}

extension Artist {
    init(
        id: String, name: String, sortName: String?, coverArt: String?,
        musicBrainzId: String?, albumCount: UInt32
    ) {
        self.init(
            id: id, name: name, sortName: sortName, coverArt: coverArt,
            musicBrainzId: musicBrainzId, albumCount: albumCount, starred: nil, userRating: nil
        )
    }
}

func playbackTrack(
    _ id: String,
    title: String? = nil,
    disc: UInt32? = 1,
    number: UInt32? = nil,
    duration: UInt32 = 180
) -> Track {
    Track(
        id: id, title: title ?? id, album: "Album", albumId: "album:1",
        artist: "Artist", artistId: "artist:1", track: number,
        discNumber: disc, year: 2026, genre: nil, coverArt: "cover:1",
        size: 0, contentType: "audio/flac", suffix: "flac",
        duration: duration, bitRate: 0, created: nil, path: nil
    )
}
