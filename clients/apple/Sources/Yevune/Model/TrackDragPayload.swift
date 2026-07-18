import CoreTransferable
import UniformTypeIdentifiers

extension UTType {
    static let yevuneTrackIdentifiers = UTType(
        exportedAs: "com.yevune.track-identifiers",
        conformingTo: .data
    )
}

struct TrackDragPayload: Codable, Hashable, Sendable, Transferable {
    let trackIDs: [String]

    static var transferRepresentation: some TransferRepresentation {
        CodableRepresentation(contentType: .yevuneTrackIdentifiers)
    }
}

enum TrackDragPolicy {
    static func payload(
        rowTrackID: String,
        selectedTrackIDs: Set<String>,
        orderedTrackIDs: [String]
    ) -> TrackDragPayload {
        guard selectedTrackIDs.contains(rowTrackID) else {
            return TrackDragPayload(trackIDs: rowTrackID.isEmpty ? [] : [rowTrackID])
        }
        return TrackDragPayload(
            trackIDs: orderedTrackIDs.filter { selectedTrackIDs.contains($0) && !$0.isEmpty }
        )
    }

    static func payload(
        rowPosition: Int,
        selectedPositions: Set<Int>,
        orderedTrackIDs: [String]
    ) -> TrackDragPayload {
        guard orderedTrackIDs.indices.contains(rowPosition) else {
            return TrackDragPayload(trackIDs: [])
        }
        guard selectedPositions.contains(rowPosition) else {
            let id = orderedTrackIDs[rowPosition]
            return TrackDragPayload(trackIDs: id.isEmpty ? [] : [id])
        }
        return TrackDragPayload(
            trackIDs: orderedTrackIDs.indices.compactMap { index in
                guard selectedPositions.contains(index) else { return nil }
                let id = orderedTrackIDs[index]
                return id.isEmpty ? nil : id
            }
        )
    }

    static func acceptedTrackIDs(
        from payloads: [TrackDragPayload],
        isMutating: Bool
    ) -> [String]? {
        guard !isMutating else { return nil }
        let ids = payloads.flatMap(\.trackIDs).filter { !$0.isEmpty }
        return ids.isEmpty ? nil : ids
    }
}
