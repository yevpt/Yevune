import XCTest
@testable import Yevune

@MainActor
final class EditorSheetLifecycleTests: XCTestCase {
    func testDismissalPolicyAllowsCleanBlocksSubmittingAndConfirmsDirty() {
        XCTAssertEqual(
            EditorSheetDismissalPolicy.request(isDirty: false, isSubmitting: false),
            .dismiss
        )
        XCTAssertEqual(
            EditorSheetDismissalPolicy.request(isDirty: true, isSubmitting: false),
            .confirmDiscard
        )
        XCTAssertEqual(
            EditorSheetDismissalPolicy.request(isDirty: false, isSubmitting: true),
            .blocked
        )
        XCTAssertEqual(
            EditorSheetDismissalPolicy.request(isDirty: true, isSubmitting: true),
            .blocked
        )
        XCTAssertTrue(EditorSheetDismissalPolicy.interactiveDismissDisabled(isDirty: true, isSubmitting: false))
        XCTAssertTrue(EditorSheetDismissalPolicy.interactiveDismissDisabled(isDirty: false, isSubmitting: true))
        XCTAssertFalse(EditorSheetDismissalPolicy.interactiveDismissDisabled(isDirty: false, isSubmitting: false))
    }

    func testSuspendedSubmissionBlocksDismissalAndDeliversSuccessExactlyOnce() async {
        let gate = EditorSubmissionGate()
        let lifecycle = EditorSheetLifecycle()
        var successCount = 0
        let submission = Task {
            await lifecycle.submit(
                operation: {
                    await gate.wait()
                    return true
                },
                onSuccess: { successCount += 1 }
            )
        }

        await gate.waitUntilEntered()
        XCTAssertTrue(lifecycle.isSubmitting)
        XCTAssertEqual(lifecycle.dismissalRequest(isDirty: false), .blocked)

        await gate.open()
        await submission.value
        await lifecycle.submit(operation: { true }, onSuccess: { successCount += 1 })

        XCTAssertFalse(lifecycle.isSubmitting)
        XCTAssertEqual(successCount, 1)
    }
}

private actor EditorSubmissionGate {
    private var entered = false
    private var entryWaiters: [CheckedContinuation<Void, Never>] = []
    private var continuation: CheckedContinuation<Void, Never>?

    func wait() async {
        entered = true
        entryWaiters.forEach { $0.resume() }
        entryWaiters.removeAll()
        await withCheckedContinuation { continuation = $0 }
    }

    func waitUntilEntered() async {
        guard !entered else { return }
        await withCheckedContinuation { entryWaiters.append($0) }
    }

    func open() {
        continuation?.resume()
        continuation = nil
    }
}
