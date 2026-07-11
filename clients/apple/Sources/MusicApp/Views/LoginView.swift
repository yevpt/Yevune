import SwiftUI

struct LoginView: View {
    @ObservedObject var model: LoginViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("MUSIC LIBRARY")
                .font(.system(.caption, design: .monospaced, weight: .bold))
                .foregroundStyle(.orange)
            Text("连接你的曲库")
                .font(.system(size: 32, weight: .semibold, design: .rounded))
            TextField("服务器地址", text: $model.server)
            TextField("用户名", text: $model.user)
            SecureField("密码", text: $model.password)
            if let errorMessage = model.errorMessage {
                Text(errorMessage).foregroundStyle(.red)
            }
            Button(model.isSubmitting ? "正在连接…" : "连接曲库") {
                Task { await model.submit() }
            }
            .buttonStyle(.borderedProminent)
            .tint(.orange)
            .disabled(model.isSubmitting || model.server.isEmpty || model.user.isEmpty)
        }
        .textFieldStyle(.roundedBorder)
        .frame(width: 360)
        .padding(36)
        .background(.indigo.opacity(0.12), in: RoundedRectangle(cornerRadius: 24))
    }
}
