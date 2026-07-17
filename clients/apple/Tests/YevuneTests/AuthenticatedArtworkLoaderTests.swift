import AppKit
import XCTest
@testable import Yevune

@MainActor
final class AuthenticatedArtworkLoaderTests: XCTestCase {
    func testSameURLNewRevisionStartsNewLoadAndSupersedesLateOldResult() async {
        let loader = SuspendedArtworkLoader()
        let model = AuthenticatedArtworkLoaderModel(loader: loader)
        let url = URL(string: "https://example.test/cover")!
        let oldRequest = AuthenticatedArtworkRequest(url: url, revision: 0)
        let newRequest = AuthenticatedArtworkRequest(url: url, revision: 1)
        let oldImage = NSImage(size: NSSize(width: 10, height: 10))
        let newImage = NSImage(size: NSSize(width: 20, height: 20))

        let oldLoad = Task { await model.load(oldRequest) }
        await loader.waitForCalls(1)
        let newLoad = Task { await model.load(newRequest) }
        await loader.waitForCalls(2)

        loader.resolveCall(0, with: oldImage)
        let oldOutcome = await oldLoad.value
        XCTAssertEqual(oldOutcome, .superseded)
        XCTAssertNil(model.image)
        XCTAssertEqual(model.state, .loading(newRequest))

        loader.resolveCall(1, with: newImage)
        let newOutcome = await newLoad.value
        XCTAssertEqual(newOutcome, .loaded)
        XCTAssertTrue(model.image === newImage)
        XCTAssertEqual(model.state, .loaded(newRequest))
    }

    func testFailedLoadPublishesNoImage() async {
        let loader = SuspendedArtworkLoader()
        let model = AuthenticatedArtworkLoaderModel(loader: loader)
        let request = AuthenticatedArtworkRequest(
            url: URL(string: "https://example.test/cover"),
            revision: 3
        )

        let load = Task { await model.load(request) }
        await loader.waitForCalls(1)
        loader.resolveCall(0, with: nil)
        let outcome = await load.value

        XCTAssertEqual(outcome, .failed)
        XCTAssertNil(model.image)
        XCTAssertEqual(model.state, .failed(request))
    }

    func testCancelledLoadReturnsSupersededWithoutPublishingImage() async {
        let loader = SuspendedArtworkLoader()
        let model = AuthenticatedArtworkLoaderModel(loader: loader)
        let request = AuthenticatedArtworkRequest(
            url: URL(string: "https://example.test/cover"),
            revision: 4
        )
        let image = NSImage(size: NSSize(width: 30, height: 30))

        let load = Task { await model.load(request) }
        await loader.waitForCalls(1)
        load.cancel()
        loader.resolveCall(0, with: image)
        let outcome = await load.value

        XCTAssertEqual(outcome, .superseded)
        XCTAssertNil(model.image)
        XCTAssertNotEqual(model.state, .loaded(request))
    }
}

@MainActor
private final class SuspendedArtworkLoader: PlaybackArtworkLoading {
    private var calls: [CheckedContinuation<NSImage?, Never>?] = []
    private var waiters: [(Int, CheckedContinuation<Void, Never>)] = []

    func load(url: URL) async -> NSImage? {
        await withCheckedContinuation { continuation in
            calls.append(continuation)
            resumeSatisfiedWaiters()
        }
    }

    func waitForCalls(_ count: Int) async {
        guard calls.count < count else { return }
        await withCheckedContinuation { waiters.append((count, $0)) }
    }

    func resolveCall(_ index: Int, with image: NSImage?) {
        calls[index]?.resume(returning: image)
        calls[index] = nil
    }

    private func resumeSatisfiedWaiters() {
        let ready = waiters.filter { $0.0 <= calls.count }
        waiters.removeAll { $0.0 <= calls.count }
        ready.forEach { $0.1.resume() }
    }
}
