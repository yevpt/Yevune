import Foundation
import YevuneCoreFFI

enum LibraryOperationErrorPresentation {
    private static let reauthenticationMessage = "权限已变化，请重新登录"
    private static let authenticatedURLPattern = #"https?://[^\s\"\)\]]+"#

    static func message(_ error: Error) -> String {
        switch error {
        case CoreError.NotAuthenticated:
            return reauthenticationMessage
        case CoreError.Server(code: 50, message: _):
            return reauthenticationMessage
        default:
            return redactingAuthenticatedURLs(in: error.localizedDescription)
        }
    }

    private static func redactingAuthenticatedURLs(in message: String) -> String {
        guard let expression = try? NSRegularExpression(pattern: authenticatedURLPattern) else {
            return message
        }
        let range = NSRange(message.startIndex..<message.endIndex, in: message)
        return expression.stringByReplacingMatches(
            in: message,
            range: range,
            withTemplate: "<已隐藏 URL>"
        )
    }
}
