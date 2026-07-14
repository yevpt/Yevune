import SwiftUI
import YevuneCoreFFI

struct AdminUsersView: View {
    @ObservedObject var model: AdminViewModel
    @ObservedObject var access: AccessControlViewModel
    @State private var isCreatingUser = false

    var body: some View {
        HSplitView {
            VStack(spacing: 0) {
                HStack {
                    TextField("搜索姓名或邮箱", text: $model.query)
                        .textFieldStyle(.roundedBorder)
                    Button { isCreatingUser = true } label: {
                        Image(systemName: "person.badge.plus")
                    }
                    .help("添加用户")
                }
                .padding(12)

                Divider()

                if let error = model.errorMessage, model.users.isEmpty, !model.isLoading {
                    ContentUnavailableView {
                        Label("无法加载用户", systemImage: "wifi.exclamationmark")
                    } description: {
                        Text(error)
                    } actions: {
                        Button("重试") { Task { await model.load() } }
                            .buttonStyle(.borderedProminent)
                    }
                } else if model.filteredUsers.isEmpty, !model.isLoading {
                    ContentUnavailableView(
                        model.query.isEmpty ? "还没有用户" : "没有匹配的用户",
                        systemImage: "person.2.slash",
                        description: Text(model.query.isEmpty ? "添加家庭成员以共享曲库。" : "尝试搜索其他姓名或邮箱。")
                    )
                } else {
                    List(model.filteredUsers, id: \.id, selection: $model.selectedUserID) { user in
                        AdminUserRow(user: user, isCurrentUser: user.name == model.currentUsername)
                            .tag(user.id)
                    }
                    .listStyle(.sidebar)
                }
            }
            .frame(minWidth: 260, idealWidth: 300, maxWidth: 360)

            Group {
                if let user = selectedUser {
                    AdminUserDetailView(user: user, model: model, access: access)
                        .id(user)
                } else {
                    ContentUnavailableView(
                        "选择一位用户",
                        systemImage: "person.crop.circle",
                        description: Text("查看账号资料、管理员权限和自定义角色。")
                    )
                }
            }
            .frame(minWidth: 460, maxWidth: .infinity, maxHeight: .infinity)
        }
        .navigationTitle("用户")
        .overlay(alignment: .top) {
            if let error = model.errorMessage, !model.users.isEmpty {
                AdminErrorBanner(message: error) {
                    Task { await model.load() }
                }
                    .padding(.top, 8)
            }
        }
        .task {
            if model.users.isEmpty { await model.load() }
            if !accessHasLoadedState, !access.isLoading { await access.load() }
        }
        .sheet(isPresented: $isCreatingUser) {
            CreateUserSheet(model: model)
        }
    }

    private var selectedUser: User? {
        model.users.first { $0.id == model.selectedUserID }
    }

    private var accessHasLoadedState: Bool {
        !access.rules.isEmpty || !access.users.isEmpty || !access.roles.isEmpty
    }
}

private struct AdminUserRow: View {
    let user: User
    let isCurrentUser: Bool

    var body: some View {
        HStack(spacing: 10) {
            ZStack {
                Circle()
                    .fill(user.admin ? Color.indigo.opacity(0.18) : Color.secondary.opacity(0.12))
                Text(user.name.prefix(1).uppercased())
                    .font(.system(.callout, design: .rounded, weight: .semibold))
                    .foregroundStyle(user.admin ? Color.indigo : Color.secondary)
            }
            .overlay {
                if user.admin {
                    Circle().stroke(Color.indigo.opacity(0.8), lineWidth: 1.5)
                }
            }
            .frame(width: 32, height: 32)

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Text(user.name).lineLimit(1)
                    if isCurrentUser {
                        Text("你")
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(.secondary)
                    }
                }
                Text(displayEmail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 4)
            if user.admin {
                Image(systemName: "checkmark.seal.fill")
                    .foregroundStyle(.indigo)
                    .help("管理员")
            }
        }
        .padding(.vertical, 3)
    }

    private var displayEmail: String {
        guard let email = user.email, !email.isEmpty else { return "未设置邮箱" }
        return email
    }
}

private struct AdminUserDetailView: View {
    let user: User
    @ObservedObject var model: AdminViewModel
    @ObservedObject var access: AccessControlViewModel
    @State private var email: String
    @State private var isAdmin: Bool
    @State private var isResettingPassword = false
    @State private var isConfirmingDelete = false

    init(user: User, model: AdminViewModel, access: AccessControlViewModel) {
        self.user = user
        self.model = model
        self.access = access
        _email = State(initialValue: user.email ?? "")
        _isAdmin = State(initialValue: user.admin)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                HStack(spacing: 14) {
                    AdminIdentityMark(user: user)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(user.name)
                            .font(.system(size: 28, weight: .semibold, design: .rounded))
                        Text(user.admin ? "管理员账号" : "家庭成员")
                            .foregroundStyle(.secondary)
                    }
                }

                GroupBox("账号资料") {
                    VStack(alignment: .leading, spacing: 14) {
                        LabeledContent("用户名") { Text(user.name).foregroundStyle(.secondary) }
                        LabeledContent("邮箱") {
                            TextField("可选", text: $email)
                                .textFieldStyle(.roundedBorder)
                                .frame(maxWidth: 320)
                        }
                        Toggle("允许管理服务器与曲库", isOn: $isAdmin)
                        if !model.canSetAdmin(user, to: isAdmin) {
                            Label(adminGuardExplanation, systemImage: "exclamationmark.shield")
                                .font(.caption)
                                .foregroundStyle(.orange)
                        }
                        HStack {
                            Button("保存更改") {
                                Task { await model.updateUser(user, email: email, admin: isAdmin) }
                            }
                            .buttonStyle(.borderedProminent)
                            .disabled(model.isMutating || !model.canSetAdmin(user, to: isAdmin))

                            Button("重置密码") { isResettingPassword = true }
                                .disabled(model.isMutating)
                        }
                    }
                    .padding(8)
                }

                GroupBox("自定义角色") {
                    VStack(alignment: .leading, spacing: 10) {
                        if model.customRoles.isEmpty {
                            Text("还没有自定义角色。可在“角色”中创建。")
                                .foregroundStyle(.secondary)
                        } else {
                            ForEach(model.customRoles, id: \.id) { role in
                                Toggle(role.name, isOn: roleBinding(role))
                                    .disabled(model.isMutating)
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(8)
                }

                GroupBox("危险操作") {
                    HStack {
                        VStack(alignment: .leading, spacing: 3) {
                            Text("删除用户").font(.headline)
                            Text(deleteExplanation).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("删除用户", role: .destructive) { isConfirmingDelete = true }
                            .disabled(model.isMutating || !model.canDelete(user))
                    }
                    .padding(8)
                }
            }
            .frame(maxWidth: 760, alignment: .leading)
            .padding(28)
        }
        .sheet(isPresented: $isResettingPassword) {
            ResetPasswordSheet(user: user, model: model)
        }
        .confirmationDialog("删除 \(user.name)？", isPresented: $isConfirmingDelete, titleVisibility: .visible) {
            Button("删除用户", role: .destructive) {
                Task { await model.deleteUser(user) }
            }
            Button("取消", role: .cancel) {}
        } message: {
            Text("该用户的账号将被永久删除，曲库中的音乐不会受影响。")
            Text("该用户会从 \(access.ruleReferenceCount(userID: user.id)) 条可见范围规则中移除。")
        }
    }

    private var deleteExplanation: String {
        if user.name == model.currentUsername { return "不能删除当前登录账号。" }
        if user.admin && !model.canDelete(user) { return "必须保留至少一个管理员。" }
        return "账号删除后无法恢复，音乐文件不会被删除。"
    }

    private var adminGuardExplanation: String {
        if user.name == model.currentUsername {
            return "当前账号不能移除自己的管理员权限。"
        }
        return "必须保留至少一个管理员。"
    }

    private func roleBinding(_ role: Role) -> Binding<Bool> {
        Binding(
            get: { user.roles.contains(role.name) },
            set: { assigned in
                Task { await model.setRole(role, assigned: assigned, for: user) }
            }
        )
    }
}

private struct AdminIdentityMark: View {
    let user: User

    var body: some View {
        ZStack {
            Circle().fill(Color.indigo.opacity(0.16))
            Text(user.name.prefix(1).uppercased())
                .font(.system(size: 26, weight: .bold, design: .rounded))
                .foregroundStyle(.indigo)
        }
        .overlay { Circle().stroke(Color.indigo.opacity(user.admin ? 0.9 : 0.25), lineWidth: user.admin ? 2 : 1) }
        .frame(width: 58, height: 58)
    }
}

private struct CreateUserSheet: View {
    @ObservedObject var model: AdminViewModel
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var email = ""
    @State private var password = ""
    @State private var confirmation = ""
    @State private var admin = false

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("添加家庭成员").font(.title2.bold())
            Form {
                TextField("用户名", text: $name)
                TextField("邮箱（可选）", text: $email)
                SecureField("初始密码", text: $password)
                SecureField("确认密码", text: $confirmation)
                Toggle("设为管理员", isOn: $admin)
            }
            HStack {
                Spacer()
                Button("取消", role: .cancel) { dismiss() }
                Button("添加用户") {
                    Task {
                        let succeeded = await model.createUser(
                            name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                            email: email.trimmingCharacters(in: .whitespacesAndNewlines),
                            password: password,
                            admin: admin
                        )
                        if succeeded { dismiss() }
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(!isValid || model.isMutating)
            }
        }
        .padding(24)
        .frame(width: 440)
        .interactiveDismissDisabled(model.isMutating)
    }

    private var isValid: Bool {
        !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !password.isEmpty
            && password == confirmation
    }
}

private struct ResetPasswordSheet: View {
    let user: User
    @ObservedObject var model: AdminViewModel
    @Environment(\.dismiss) private var dismiss
    @State private var password = ""
    @State private var confirmation = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("重置 \(user.name) 的密码").font(.title2.bold())
            SecureField("新密码", text: $password)
            SecureField("确认新密码", text: $confirmation)
            HStack {
                Spacer()
                Button("取消", role: .cancel) { dismiss() }
                Button("更新密码") {
                    Task {
                        let succeeded = await model.changePassword(for: user, password: password)
                        if succeeded { dismiss() }
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(password.isEmpty || password != confirmation || model.isMutating)
            }
        }
        .padding(24)
        .frame(width: 420)
        .interactiveDismissDisabled(model.isMutating)
    }
}

struct AdminErrorBanner: View {
    let message: String
    let retry: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Label(message, systemImage: "exclamationmark.triangle.fill")
            Divider().frame(height: 16)
            Button("重新加载", action: retry)
                .buttonStyle(.borderless)
                .fontWeight(.semibold)
        }
            .font(.callout)
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .background(.regularMaterial, in: Capsule())
            .overlay { Capsule().stroke(Color.orange.opacity(0.45)) }
            .shadow(color: .black.opacity(0.08), radius: 10, y: 4)
    }
}
