import YevuneCoreFFI

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
