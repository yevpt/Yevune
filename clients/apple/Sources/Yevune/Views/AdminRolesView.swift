import SwiftUI
import YevuneCoreFFI

struct AdminRolesView: View {
    @ObservedObject var model: AdminViewModel
    @ObservedObject var access: AccessControlViewModel
    @State private var isCreatingRole = false

    var body: some View {
        HSplitView {
            VStack(spacing: 0) {
                if let error = model.errorMessage, model.roles.isEmpty, !model.isLoading {
                    ContentUnavailableView {
                        Label("无法加载角色", systemImage: "wifi.exclamationmark")
                    } description: {
                        Text(error)
                    } actions: {
                        Button("重试") { Task { await model.load() } }
                            .buttonStyle(.borderedProminent)
                    }
                } else {
                    List(selection: $model.selectedRoleID) {
                        Section("内建角色") {
                            ForEach(model.roles.filter(\.isBuiltin), id: \.id) { role in
                                RoleRow(role: role, memberCount: model.affectedUserCount(for: role))
                                    .tag(role.id)
                            }
                        }
                        Section("自定义角色") {
                            ForEach(model.customRoles, id: \.id) { role in
                                RoleRow(role: role, memberCount: model.affectedUserCount(for: role))
                                    .tag(role.id)
                            }
                            if model.customRoles.isEmpty {
                                Text("还没有自定义角色")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                    .listStyle(.sidebar)
                }

                Divider()
                Button { isCreatingRole = true } label: {
                    Label("新建角色", systemImage: "plus")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderless)
                .padding(12)
                .disabled(model.isMutating)
            }
            .frame(minWidth: 260, idealWidth: 300, maxWidth: 360)

            Group {
                if let role = selectedRole {
                    AdminRoleDetailView(role: role, model: model, access: access)
                        .id(role)
                } else {
                    ContentUnavailableView(
                        "选择一个角色",
                        systemImage: "person.badge.key",
                        description: Text("查看角色类型和拥有该角色的家庭成员。")
                    )
                }
            }
            .frame(minWidth: 460, maxWidth: .infinity, maxHeight: .infinity)
        }
        .navigationTitle("角色")
        .overlay(alignment: .top) {
            if (model.errorMessage != nil && !model.roles.isEmpty) || access.errorMessage != nil {
                VStack(spacing: 8) {
                    if let error = model.errorMessage, !model.roles.isEmpty {
                        AdminErrorBanner(message: error) {
                            Task { await model.load() }
                        }
                    }
                    if let error = access.errorMessage {
                        AdminErrorBanner(message: "可见范围：\(error)") {
                            Task { await access.load() }
                        }
                    }
                }
                .padding(.top, 8)
            }
        }
        .task {
            if model.roles.isEmpty { await model.load() }
            if !access.hasLoadedSuccessfully, !access.isLoading { await access.load() }
        }
        .sheet(isPresented: $isCreatingRole) {
            CreateRoleSheet(model: model)
        }
    }

    private var selectedRole: Role? {
        model.roles.first { $0.id == model.selectedRoleID }
    }
}

private struct RoleRow: View {
    let role: Role
    let memberCount: Int

    var body: some View {
        HStack(spacing: 10) {
            ZStack {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(role.isBuiltin ? Color.secondary.opacity(0.12) : Color.indigo.opacity(0.16))
                Image(systemName: role.isBuiltin ? "lock.fill" : "person.badge.key.fill")
                    .font(.caption)
                    .foregroundStyle(role.isBuiltin ? Color.secondary : Color.indigo)
            }
            .frame(width: 32, height: 32)

            VStack(alignment: .leading, spacing: 2) {
                Text(role.name).lineLimit(1)
                Text("\(memberCount) 位成员")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if role.isBuiltin {
                Text("系统")
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 3)
    }
}

private struct AdminRoleDetailView: View {
    let role: Role
    @ObservedObject var model: AdminViewModel
    @ObservedObject var access: AccessControlViewModel
    @State private var isConfirmingDelete = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                HStack(spacing: 14) {
                    ZStack {
                        RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .fill(role.isBuiltin ? Color.secondary.opacity(0.12) : Color.indigo.opacity(0.16))
                        Image(systemName: role.isBuiltin ? "lock.shield.fill" : "person.badge.key.fill")
                            .font(.title2)
                            .foregroundStyle(role.isBuiltin ? Color.secondary : Color.indigo)
                    }
                    .frame(width: 58, height: 58)

                    VStack(alignment: .leading, spacing: 3) {
                        Text(role.name)
                            .font(.system(size: 28, weight: .semibold, design: .rounded))
                        Text(role.isBuiltin ? "系统内建角色" : "自定义访问角色")
                            .foregroundStyle(.secondary)
                    }
                }

                GroupBox("成员") {
                    VStack(alignment: .leading, spacing: 0) {
                        if members.isEmpty {
                            Text("还没有用户拥有此角色。")
                                .foregroundStyle(.secondary)
                                .padding(8)
                        } else {
                            ForEach(members, id: \.id) { user in
                                HStack(spacing: 10) {
                                    Image(systemName: "person.crop.circle.fill")
                                        .foregroundStyle(user.admin ? Color.indigo : Color.secondary)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(user.name)
                                        if let email = user.email, !email.isEmpty {
                                            Text(email).font(.caption).foregroundStyle(.secondary)
                                        }
                                    }
                                    Spacer()
                                    if user.admin {
                                        Text("管理员").font(.caption).foregroundStyle(.secondary)
                                    }
                                }
                                .padding(.vertical, 9)
                                if user.id != members.last?.id { Divider() }
                            }
                        }
                    }
                    .padding(.horizontal, 8)
                }

                GroupBox(role.isBuiltin ? "系统角色" : "危险操作") {
                    HStack {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(role.isBuiltin ? "此角色由系统维护" : "删除角色")
                                .font(.headline)
                            Text(role.isBuiltin
                                 ? "系统角色不可删除或重命名。"
                                 : "将从 \(model.affectedUserCount(for: role)) 位用户移除此角色。")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        if !role.isBuiltin {
                            Button("删除角色", role: .destructive) { isConfirmingDelete = true }
                                .disabled(
                                    model.isMutating
                                        || !model.canDelete(role)
                                        || !access.hasLoadedSuccessfully
                                        || access.isLoading
                                )
                        }
                    }
                    .padding(8)
                }
            }
            .frame(maxWidth: 760, alignment: .leading)
            .padding(28)
        }
        .confirmationDialog("删除 \(role.name)？", isPresented: $isConfirmingDelete, titleVisibility: .visible) {
            Button("删除角色", role: .destructive) {
                Task {
                    let succeeded = await model.deleteRole(role)
                    await access.refreshAfterPrincipalDeletion(succeeded: succeeded)
                }
            }
            Button("取消", role: .cancel) {}
        } message: {
            Text("将从 \(model.affectedUserCount(for: role)) 位用户移除此角色。")
            Text("该角色会从 \(access.ruleReferenceCount(roleID: role.id)) 条可见范围规则中移除。")
        }
    }

    private var members: [User] {
        model.users.filter { $0.roles.contains(role.name) }
    }
}

private struct CreateRoleSheet: View {
    @ObservedObject var model: AdminViewModel
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("新建访问角色").font(.title2.bold())
            Text("角色用于把一组曲库访问规则分配给家庭成员。")
                .foregroundStyle(.secondary)
            TextField("角色名称", text: $name)
                .textFieldStyle(.roundedBorder)
            HStack {
                Spacer()
                Button("取消", role: .cancel) { dismiss() }
                Button("创建角色") {
                    Task {
                        let succeeded = await model.createRole(name: trimmedName)
                        if succeeded { dismiss() }
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(trimmedName.isEmpty || model.isMutating)
            }
        }
        .padding(24)
        .frame(width: 420)
        .interactiveDismissDisabled(model.isMutating)
    }

    private var trimmedName: String {
        name.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
