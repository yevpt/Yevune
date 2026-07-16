import Foundation
import YevuneCoreFFI

enum BatchFieldMode: Equatable {
    case keep
    case set(String)
    case clear
}

struct TagDraftValidation: Equatable {
    var title: String?
    var year: String?
    var track: String?
    var discNumber: String?

    var isValid: Bool {
        [title, year, track, discNumber].allSatisfy { $0 == nil }
    }
}

struct TagDraft {
    private let original: OriginalTags

    var title: String
    var album: String
    var artist: String
    var genre: String
    var year: String
    var track: String
    var discNumber: String

    init(track: Track) {
        original = OriginalTags(track: track)
        title = track.title
        album = track.album ?? ""
        artist = track.artist ?? ""
        genre = track.genre ?? ""
        year = track.year.map(String.init) ?? ""
        self.track = track.track.map(String.init) ?? ""
        discNumber = track.discNumber.map(String.init) ?? ""
    }

    init() {
        original = OriginalTags()
        title = ""
        album = ""
        artist = ""
        genre = ""
        year = ""
        track = ""
        discNumber = ""
    }

    var validation: TagDraftValidation {
        TagDraftValidation(
            title: trimmed(title).isEmpty ? "标题不能为空" : nil,
            year: numericError(year, maximum: 9_999, fieldName: "年份"),
            track: numericError(track, maximum: 999, fieldName: "曲序"),
            discNumber: numericError(discNumber, maximum: 999, fieldName: "碟序")
        )
    }

    var isDirty: Bool {
        normalizedTitle != original.title
            || textIntent(album, original: original.album).isChanged
            || textIntent(artist, original: original.artist).isChanged
            || textIntent(genre, original: original.genre).isChanged
            || numberIntent(year, original: original.year).isChanged
            || numberIntent(track, original: original.track).isChanged
            || numberIntent(discNumber, original: original.discNumber).isChanged
    }

    func makeUpdate() -> TagUpdate? {
        guard validation.isValid else { return nil }

        let albumIntent = textIntent(album, original: original.album)
        let artistIntent = textIntent(artist, original: original.artist)
        let genreIntent = textIntent(genre, original: original.genre)
        let yearIntent = numberIntent(year, original: original.year)
        let trackIntent = numberIntent(track, original: original.track)
        let discIntent = numberIntent(discNumber, original: original.discNumber)
        var clearFields: [TagField] = []

        appendClear(.album, for: albumIntent, to: &clearFields)
        appendClear(.artist, for: artistIntent, to: &clearFields)
        appendClear(.genre, for: genreIntent, to: &clearFields)
        appendClear(.year, for: yearIntent, to: &clearFields)
        appendClear(.track, for: trackIntent, to: &clearFields)
        appendClear(.discNumber, for: discIntent, to: &clearFields)

        let title = normalizedTitle == original.title ? nil : normalizedTitle
        guard title != nil
            || albumIntent.isChanged || artistIntent.isChanged || genreIntent.isChanged
            || yearIntent.isChanged || trackIntent.isChanged || discIntent.isChanged
        else { return nil }

        return TagUpdate(
            title: title,
            album: albumIntent.value,
            artist: artistIntent.value,
            genre: genreIntent.value,
            year: yearIntent.number,
            track: trackIntent.number,
            discNumber: discIntent.number,
            clearFields: clearFields
        )
    }

    private var normalizedTitle: String { trimmed(title) }
}

struct BatchTagDraft {
    var album: BatchFieldMode = .keep
    var artist: BatchFieldMode = .keep
    var genre: BatchFieldMode = .keep
    var year: BatchFieldMode = .keep

    var validation: TagDraftValidation {
        TagDraftValidation(
            title: nil,
            year: batchYearError,
            track: nil,
            discNumber: nil
        )
    }

    func makeUpdate() -> TagUpdate? {
        guard validation.isValid,
              textModeIsValid(album), textModeIsValid(artist), textModeIsValid(genre)
        else { return nil }
        guard [album, artist, genre, year].contains(where: { $0 != .keep }) else { return nil }

        var clearFields: [TagField] = []
        appendBatchClear(.album, mode: album, to: &clearFields)
        appendBatchClear(.artist, mode: artist, to: &clearFields)
        appendBatchClear(.genre, mode: genre, to: &clearFields)
        appendBatchClear(.year, mode: year, to: &clearFields)

        return TagUpdate(
            title: nil,
            album: batchTextValue(album),
            artist: batchTextValue(artist),
            genre: batchTextValue(genre),
            year: batchYearValue,
            track: nil,
            discNumber: nil,
            clearFields: clearFields
        )
    }

    private var batchYearError: String? {
        guard case let .set(value) = year else { return nil }
        return numericError(value, maximum: 9_999, fieldName: "年份", allowsBlank: false)
    }

    private var batchYearValue: UInt32? {
        guard case let .set(value) = year else { return nil }
        return UInt32(trimmed(value))
    }
}

private struct OriginalTags {
    let title: String
    let album: String?
    let artist: String?
    let genre: String?
    let year: UInt32?
    let track: UInt32?
    let discNumber: UInt32?

    init(track: Track) {
        title = trimmed(track.title)
        album = track.album.map(trimmed)
        artist = track.artist.map(trimmed)
        genre = track.genre.map(trimmed)
        year = track.year
        self.track = track.track
        discNumber = track.discNumber
    }

    init() {
        title = ""
        album = nil
        artist = nil
        genre = nil
        year = nil
        track = nil
        discNumber = nil
    }
}

private enum FieldIntent {
    case keep
    case setText(String)
    case setNumber(UInt32)
    case clear
    case invalid

    var isChanged: Bool {
        if case .keep = self { return false }
        return true
    }

    var value: String? {
        guard case let .setText(value) = self else { return nil }
        return value
    }

    var number: UInt32? {
        guard case let .setNumber(value) = self else { return nil }
        return value
    }
}

private func trimmed(_ value: String) -> String {
    value.trimmingCharacters(in: .whitespacesAndNewlines)
}

private func numericError(
    _ text: String,
    maximum: UInt32,
    fieldName: String,
    allowsBlank: Bool = true
) -> String? {
    let value = trimmed(text)
    if value.isEmpty { return allowsBlank ? nil : "\(fieldName)不能为空" }
    guard let number = UInt32(value), (1 ... maximum).contains(number) else {
        return "\(fieldName)必须为 1...\(maximum) 的整数"
    }
    return nil
}

private func textIntent(_ text: String, original: String?) -> FieldIntent {
    let value = trimmed(text)
    if value.isEmpty {
        guard let original else { return .keep }
        return original.isEmpty ? .keep : .clear
    }
    return value == original ? .keep : .setText(value)
}

private func numberIntent(_ text: String, original: UInt32?) -> FieldIntent {
    let value = trimmed(text)
    if value.isEmpty { return original == nil ? .keep : .clear }
    guard let number = UInt32(value) else { return .invalid }
    return number == original ? .keep : .setNumber(number)
}

private func appendClear(_ field: TagField, for intent: FieldIntent, to fields: inout [TagField]) {
    if case .clear = intent { fields.append(field) }
}

private func textModeIsValid(_ mode: BatchFieldMode) -> Bool {
    guard case let .set(value) = mode else { return true }
    return !trimmed(value).isEmpty
}

private func batchTextValue(_ mode: BatchFieldMode) -> String? {
    guard case let .set(value) = mode else { return nil }
    return trimmed(value)
}

private func appendBatchClear(_ field: TagField, mode: BatchFieldMode, to fields: inout [TagField]) {
    if case .clear = mode { fields.append(field) }
}
