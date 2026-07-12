import AppKit

@MainActor
protocol ApplicationActivating: AnyObject {
    func setRegularActivationPolicy()
    func activateIgnoringOtherApps()
}

extension NSApplication: ApplicationActivating {
    func setRegularActivationPolicy() {
        setActivationPolicy(.regular)
    }

    func activateIgnoringOtherApps() {
        activate(ignoringOtherApps: true)
    }
}

@MainActor
enum LaunchCoordinator {
    static func activate(_ application: any ApplicationActivating) {
        application.setRegularActivationPolicy()
        application.activateIgnoringOtherApps()
    }
}

final class ApplicationDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        LaunchCoordinator.activate(NSApplication.shared)
    }
}
