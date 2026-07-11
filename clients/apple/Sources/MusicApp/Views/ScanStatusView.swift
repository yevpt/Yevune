import SwiftUI

struct ScanStatusView: View {
    @ObservedObject var model: ScanStatusViewModel

    var body: some View {
        VStack(spacing: 16) {
            Text(model.status?.scanning == true ? "正在扫描曲库" : "扫描空闲")
                .font(.title2.weight(.semibold))
            Text("已处理 \(model.status?.count ?? 0) 项")
                .monospacedDigit()
            if let errorMessage = model.errorMessage { Text(errorMessage).foregroundStyle(.red) }
            Button("开始扫描") { Task { await model.start() } }
        }
        .padding()
        .task {
            while !Task.isCancelled {
                await model.refresh()
                try? await Task.sleep(for: .seconds(1))
            }
        }
    }
}
