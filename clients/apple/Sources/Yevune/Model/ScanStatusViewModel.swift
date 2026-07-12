import YevuneCoreFFI
import Foundation

@MainActor
final class ScanStatusViewModel: ObservableObject {
    @Published private(set) var status: ScanStatus?
    @Published private(set) var errorMessage: String?

    private let client: any MusicClientProviding

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func start() async {
        errorMessage = nil
        do { status = try await client.startScan() }
        catch { errorMessage = error.localizedDescription }
    }

    func refresh() async {
        do { status = try await client.scanStatus() }
        catch { errorMessage = error.localizedDescription }
    }
}
