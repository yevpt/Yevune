import Foundation

enum EditorSheetDismissalRequest: Equatable {
    case dismiss
    case confirmDiscard
    case blocked
}

enum EditorSheetDismissalPolicy {
    static func request(isDirty: Bool, isSubmitting: Bool) -> EditorSheetDismissalRequest {
        if isSubmitting { return .blocked }
        return isDirty ? .confirmDiscard : .dismiss
    }

    static func interactiveDismissDisabled(isDirty: Bool, isSubmitting: Bool) -> Bool {
        isDirty || isSubmitting
    }
}

@MainActor
final class EditorSheetLifecycle: ObservableObject {
    @Published private(set) var isSubmitting = false
    private var didDeliverSuccess = false

    func dismissalRequest(isDirty: Bool) -> EditorSheetDismissalRequest {
        EditorSheetDismissalPolicy.request(isDirty: isDirty, isSubmitting: isSubmitting)
    }

    func submit(
        operation: @escaping @MainActor () async -> Bool,
        onSuccess: @escaping @MainActor () -> Void
    ) async {
        guard !isSubmitting, !didDeliverSuccess else { return }
        isSubmitting = true
        let succeeded = await operation()
        isSubmitting = false
        guard succeeded, !didDeliverSuccess else { return }
        didDeliverSuccess = true
        onSuccess()
    }
}
