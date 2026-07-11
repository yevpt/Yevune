import XCTest
@testable import MusicApp

@MainActor
final class LoginViewModelTests: XCTestCase {
    func testProductionBridgeConformsToViewModelProtocol() {
        let client: any MusicClientProviding = CoreMusicClient()
        XCTAssertNotNil(client as AnyObject)
    }

    func testSubmitPublishesAuthenticatedSession() async {
        let client = FakeMusicClient()
        let model = LoginViewModel(client: client)
        model.server = "http://music.local:4533"
        model.user = "admin"
        model.password = "secret"

        await model.submit()

        XCTAssertEqual(model.session, .init(server: "http://music.local:4533", user: "admin"))
        XCTAssertNil(model.errorMessage)
        XCTAssertFalse(model.isSubmitting)
    }
}

private actor FakeMusicClient: MusicClientProviding {
    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user)
    }
}
