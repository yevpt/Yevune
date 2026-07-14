import SwiftUI
import YevuneCoreFFI

struct AccessRuleEditorView: View {
    let target: AccessScopeTarget
    @ObservedObject var model: AccessControlViewModel
    let onComplete: () -> Void

    @State private var selectedUserIDs: Set<String>
    @State private var selectedRoleIDs: Set<String>
    @State private var isConfirmingEmptyGrant = false
    @State private var isConfirmingRestore = false

    init(
        target: AccessScopeTarget,
        model: AccessControlViewModel,
        onComplete: @escaping () -> Void
    ) {
        self.target = target
        self.model = model
        self.onComplete = onComplete

        let grants = model.rule(for: target)?.grants ?? []
        _selectedUserIDs = State(
            initialValue: Set(
                grants.filter { $0.principalType == .user }.map(\.id)
            )
        )
        _selectedRoleIDs = State(
            initialValue: Set(
                grants.filter { $0.principalType == .role }.map(\.id)
            )
        )
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                targetSummary
                visibilitySummary
                memberSelection
                roleSelection
                priorityExplanation
                actions
            }
            .frame(maxWidth: 760, alignment: .leading)
            .padding(28)
        }
        .confirmationDialog(
            "仅管理员可见？",
            isPresented: $isConfirmingEmptyGrant,
            titleVisibility: .visible
        ) {
            Button("保存为仅管理员可见", role: .destructive) {
                save(grants: [])
            }
            Button("取消", role: .cancel) {}
        } message: {
            Text("除管理员外，所有家庭成员都将看不到此内容。")
        }
        .confirmationDialog(
            "恢复全家可见？",
            isPresented: $isConfirmingRestore,
            titleVisibility: .visible
        ) {
            Button("恢复全家可见", role: .destructive) {
                restoreFamilyVisibility()
            }
            Button("取消", role: .cancel) {}
        } message: {
            Text("这项限制将被移除；若没有更具体的限制，全家都能看到此内容。")
        }
    }

    private var targetSummary: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                AccessScopeBadge(scopeType: target.scopeType)
                Text(currentVisibilityLabel)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            Text(target.name)
                .font(.title2.weight(.semibold))
                .textSelection(.enabled)
            if let context = target.context, !context.isEmpty {
                Text(context)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        }
    }

    private var visibilitySummary: some View {
        GroupBox("可见范围") {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: selectionIsEmpty ? "lock.shield" : "person.2")
                    .font(.title2)
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 4) {
                    Text(selectionIsEmpty ? "保存后仅管理员可见" : "保存后仅所选成员和角色可见")
                        .font(.headline)
                    Text("成员获得直接可见权限，或属于所选角色，任一条件满足即可看见。管理员始终可见。")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(8)
        }
    }

    private var memberSelection: some View {
        GroupBox("家庭成员") {
            VStack(alignment: .leading, spacing: 10) {
                if model.assignableUsers.isEmpty {
                    Text("没有可单独选择的家庭成员。")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(model.assignableUsers.sorted(by: userSort), id: \.id) { user in
                        Toggle(isOn: selectionBinding(forUserID: user.id)) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(user.name)
                                if let email = user.email, !email.isEmpty {
                                    Text(email)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                        .disabled(model.isMutating)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(8)
        }
    }

    private var roleSelection: some View {
        GroupBox("角色") {
            VStack(alignment: .leading, spacing: 10) {
                if model.assignableRoles.isEmpty {
                    Text("没有可选择的角色。")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(model.assignableRoles.sorted(by: roleSort), id: \.id) { role in
                        Toggle(role.name, isOn: selectionBinding(forRoleID: role.id))
                            .disabled(model.isMutating)
                    }
                }
                Label("管理员账号与管理员角色始终可见，因此不列入选择。", systemImage: "checkmark.shield")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(8)
        }
    }

    private var priorityExplanation: some View {
        Label {
            Text("更具体的可见范围优先：曲目高于专辑，专辑高于艺人，艺人高于流派。")
        } icon: {
            Image(systemName: "arrow.down.to.line.compact")
        }
        .font(.caption)
        .foregroundStyle(.secondary)
    }

    private var actions: some View {
        HStack {
            Button("保存可见范围") {
                let grants = selectedPrincipals
                if model.requiresEmptyGrantConfirmation(grants) {
                    isConfirmingEmptyGrant = true
                } else {
                    save(grants: grants)
                }
            }
            .buttonStyle(.borderedProminent)
            .keyboardShortcut(.defaultAction)
            .disabled(model.isMutating || model.isLoading)

            if model.rule(for: target) != nil {
                Button("恢复全家可见", role: .destructive) {
                    isConfirmingRestore = true
                }
                .disabled(model.isMutating || model.isLoading)
            }

            if model.isMutating || model.isLoading {
                ProgressView()
                    .controlSize(.small)
                    .accessibilityLabel("正在更新可见范围")
            }
        }
    }

    private var selectedPrincipals: [Principal] {
        selectedUserIDs.sorted().map { Principal(principalType: .user, id: $0) }
            + selectedRoleIDs.sorted().map { Principal(principalType: .role, id: $0) }
    }

    private var selectionIsEmpty: Bool {
        selectedUserIDs.isEmpty && selectedRoleIDs.isEmpty
    }

    private var currentVisibilityLabel: String {
        guard let rule = model.rule(for: target) else { return "全家可见" }
        return rule.grants.isEmpty ? "仅管理员可见" : "所选可见"
    }

    private func selectionBinding(forUserID id: String) -> Binding<Bool> {
        Binding(
            get: { selectedUserIDs.contains(id) },
            set: { selected in
                if selected { selectedUserIDs.insert(id) } else { selectedUserIDs.remove(id) }
            }
        )
    }

    private func selectionBinding(forRoleID id: String) -> Binding<Bool> {
        Binding(
            get: { selectedRoleIDs.contains(id) },
            set: { selected in
                if selected { selectedRoleIDs.insert(id) } else { selectedRoleIDs.remove(id) }
            }
        )
    }

    private func userSort(_ lhs: User, _ rhs: User) -> Bool {
        lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
    }

    private func roleSort(_ lhs: Role, _ rhs: Role) -> Bool {
        lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
    }

    private func save(grants: [Principal]) {
        Task {
            if await model.saveRule(target: target, grants: grants) {
                onComplete()
            }
        }
    }

    private func restoreFamilyVisibility() {
        guard let rule = model.rule(for: target) else { return }
        Task {
            if await model.restoreFamilyVisibility(ruleID: rule.id) {
                onComplete()
            }
        }
    }
}

struct AccessScopeBadge: View {
    let scopeType: ScopeType

    var body: some View {
        Text(scopeType.accessDisplayName)
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(.quaternary, in: Capsule())
            .accessibilityLabel("作用域：\(scopeType.accessDisplayName)")
    }
}

extension ScopeType {
    var accessDisplayName: String {
        switch self {
        case .track: "曲目"
        case .album: "专辑"
        case .artist: "艺人"
        case .genre: "流派"
        }
    }

    var accessSystemImage: String {
        switch self {
        case .track: "music.note"
        case .album: "square.stack"
        case .artist: "music.mic"
        case .genre: "guitars"
        }
    }

    static var accessDisplayOrder: [ScopeType] {
        [.track, .album, .artist, .genre]
    }
}
