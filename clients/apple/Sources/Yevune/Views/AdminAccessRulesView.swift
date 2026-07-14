import SwiftUI
import YevuneCoreFFI

struct AdminAccessRulesView: View {
    @ObservedObject var model: AccessControlViewModel
    @State private var isAddingRestriction = false

    var body: some View {
        HSplitView {
            ruleIndex
                .frame(minWidth: 280, idealWidth: 320, maxWidth: 380)

            detailPane
                .frame(minWidth: 480, maxWidth: .infinity, maxHeight: .infinity)
        }
        .navigationTitle("访问控制")
        .overlay(alignment: .top) {
            if let error = model.errorMessage, hasLoadedState {
                AdminErrorBanner(message: error) {
                    Task { await model.load() }
                }
                .padding(.top, 8)
            }
        }
        .task {
            if !hasLoadedState {
                await model.load()
            }
        }
        .sheet(isPresented: $isAddingRestriction) {
            AddAccessRestrictionSheet(model: model)
        }
    }

    private var ruleIndex: some View {
        VStack(spacing: 0) {
            VStack(spacing: 10) {
                TextField("搜索名称或标识", text: $model.query)
                    .textFieldStyle(.roundedBorder)

                Picker("作用域", selection: $model.scopeFilter) {
                    Text("全部").tag(nil as ScopeType?)
                    ForEach(ScopeType.accessDisplayOrder, id: \.self) { scope in
                        Text(scope.accessDisplayName).tag(scope as ScopeType?)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }
            .padding(12)

            Divider()

            Group {
                if let error = model.errorMessage, !hasLoadedState, !model.isLoading {
                    ContentUnavailableView {
                        Label("无法加载可见范围", systemImage: "wifi.exclamationmark")
                    } description: {
                        Text(error)
                    } actions: {
                        Button("重试") { Task { await model.load() } }
                            .buttonStyle(.borderedProminent)
                    }
                } else if model.isLoading && !hasLoadedState {
                    ProgressView("正在加载可见范围…")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if model.filteredRules.isEmpty {
                    ContentUnavailableView(
                        model.rules.isEmpty ? "还没有限制" : "没有匹配的限制",
                        systemImage: model.rules.isEmpty ? "music.note.house" : "magnifyingglass",
                        description: Text(
                            model.rules.isEmpty
                                ? "曲库默认全家可见。"
                                : "尝试更改名称或作用域筛选。"
                        )
                    )
                } else {
                    List(selection: $model.selectedRuleID) {
                        ForEach(ScopeType.accessDisplayOrder, id: \.self) { scope in
                            let rules = rules(for: scope)
                            if !rules.isEmpty {
                                Section(scope.accessDisplayName) {
                                    ForEach(rules, id: \.id) { rule in
                                        AccessRuleRow(rule: rule)
                                            .tag(rule.id)
                                    }
                                }
                            }
                        }
                    }
                    .listStyle(.sidebar)
                }
            }

            Divider()

            Button {
                isAddingRestriction = true
            } label: {
                Label("添加限制", systemImage: "plus")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderless)
            .padding(12)
            .disabled(model.isMutating || model.isLoading)
            .help("为曲目、专辑、艺人或流派设置可见范围")
        }
    }

    @ViewBuilder
    private var detailPane: some View {
        if let rule = selectedRule {
            let target = AccessScopeTarget(
                scopeType: rule.scopeType,
                id: rule.scopeId,
                name: rule.scopeName ?? "对象已不存在",
                context: rule.scopeName == nil ? rule.scopeId : nil
            )
            AccessRuleEditorView(target: target, model: model) {
                model.selectedRuleID = model.rule(for: target)?.id
            }
            .id(rule.id)
        } else if model.rules.isEmpty, model.errorMessage == nil, !model.isLoading {
            ContentUnavailableView {
                Label("曲库默认全家可见", systemImage: "music.note.house")
            } description: {
                Text("只有需要限制的内容才会出现在这里。")
            } actions: {
                Button("添加限制") { isAddingRestriction = true }
                    .buttonStyle(.borderedProminent)
            }
        } else {
            ContentUnavailableView(
                "选择一项限制",
                systemImage: "sidebar.left",
                description: Text("审计并编辑谁能看见这项音乐内容。")
            )
        }
    }

    private var selectedRule: AccessRule? {
        model.rules.first { $0.id == model.selectedRuleID }
    }

    private var hasLoadedState: Bool {
        !model.rules.isEmpty || !model.users.isEmpty || !model.roles.isEmpty
    }

    private func rules(for scope: ScopeType) -> [AccessRule] {
        model.filteredRules
            .filter { $0.scopeType == scope }
            .sorted {
                ($0.scopeName ?? $0.scopeId).localizedStandardCompare($1.scopeName ?? $1.scopeId)
                    == .orderedAscending
            }
    }
}

private struct AccessRuleRow: View {
    let rule: AccessRule

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: rule.scopeType.accessSystemImage)
                .frame(width: 22)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 4) {
                Text(rule.scopeName ?? "对象已不存在")
                    .font(.body)
                    .lineLimit(1)
                HStack(spacing: 6) {
                    AccessScopeBadge(scopeType: rule.scopeType)
                    Text(visibilityLabel)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }

    private var visibilityLabel: String {
        if rule.grants.isEmpty { return "仅管理员可见" }
        return "所选可见 · \(rule.grants.count) 个成员或角色"
    }
}

private struct AddAccessRestrictionSheet: View {
    @ObservedObject var model: AccessControlViewModel
    @Environment(\.dismiss) private var dismiss
    @State private var scopeType: ScopeType = .track
    @State private var query = ""
    @State private var selectedTarget: AccessScopeTarget?

    var body: some View {
        VStack(spacing: 0) {
            if let target = selectedTarget {
                HStack {
                    Button {
                        selectedTarget = nil
                    } label: {
                        Label("返回搜索", systemImage: "chevron.left")
                    }
                    .help("返回目标搜索")
                    Spacer()
                    Button("取消", role: .cancel) { dismiss() }
                }
                .padding(16)
                Divider()

                AccessRuleEditorView(target: target, model: model) {
                    dismiss()
                }
                .id(target)
            } else {
                searchPane
            }
        }
        .frame(minWidth: 560, minHeight: 560)
        .interactiveDismissDisabled(model.isMutating)
    }

    private var searchPane: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text("添加可见范围限制")
                        .font(.title2.weight(.semibold))
                    Text("先选择音乐资料类型，再查找要限制的对象。")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("取消", role: .cancel) { dismiss() }
            }

            Picker("作用域", selection: $scopeType) {
                ForEach(ScopeType.accessDisplayOrder, id: \.self) { scope in
                    Text(scope.accessDisplayName).tag(scope)
                }
            }
            .pickerStyle(.segmented)
            .onChange(of: scopeType) {
                query = ""
                Task { await model.searchTargets(scopeType: scopeType, query: "") }
            }

            HStack {
                TextField("搜索\(scopeType.accessDisplayName)", text: $query)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(search)
                Button("搜索", action: search)
                    .buttonStyle(.borderedProminent)
                    .disabled(trimmedQuery.isEmpty || model.isSearching)
            }

            if let error = model.errorMessage {
                HStack(alignment: .firstTextBaseline) {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("重试", action: search)
                        .disabled(trimmedQuery.isEmpty || model.isSearching)
                }
                .padding(10)
                .background(.quaternary, in: RoundedRectangle(cornerRadius: 8))
            }

            Group {
                if model.isSearching {
                    ProgressView("正在搜索…")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if model.targetResults.isEmpty {
                    ContentUnavailableView(
                        trimmedQuery.isEmpty ? "查找音乐资料" : "没有匹配结果",
                        systemImage: "magnifyingglass",
                        description: Text(
                            trimmedQuery.isEmpty
                                ? "输入名称以设置可见范围。"
                                : "尝试其他搜索词。"
                        )
                    )
                } else {
                    List(model.targetResults) { target in
                        Button {
                            selectedTarget = target
                        } label: {
                            TargetSearchRow(
                                target: target,
                                isRestricted: model.rule(for: target) != nil
                            )
                        }
                        .buttonStyle(.plain)
                    }
                    .listStyle(.inset)
                }
            }
            .frame(maxHeight: .infinity)
        }
        .padding(24)
    }

    private var trimmedQuery: String {
        query.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func search() {
        guard !trimmedQuery.isEmpty else { return }
        Task { await model.searchTargets(scopeType: scopeType, query: query) }
    }
}

private struct TargetSearchRow: View {
    let target: AccessScopeTarget
    let isRestricted: Bool

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: target.scopeType.accessSystemImage)
                .foregroundStyle(.secondary)
                .frame(width: 22)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                Text(target.name)
                    .font(.body)
                if let context = target.context, !context.isEmpty {
                    Text(context)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer()
            if isRestricted {
                Text("已限制")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            Image(systemName: "chevron.right")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .accessibilityHidden(true)
        }
        .contentShape(Rectangle())
        .padding(.vertical, 5)
        .accessibilityElement(children: .combine)
    }
}
