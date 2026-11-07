import Darwin
import SwiftUI
import WidgetKit

struct TimerItem: Decodable {
    let title: String
    let startedAt: Double
}

struct TimerPayload: Decodable {
    let version: Int
    let updatedAt: Double
    let total: Int
    let timers: [TimerItem]
}

enum TimerStore {
    static func realHome() -> String {
        if let pw = getpwuid(getuid()), let dir = pw.pointee.pw_dir {
            return String(cString: dir)
        }
        return NSHomeDirectory()
    }

    static func load() -> TimerPayload? {
        let path = realHome() + "/Library/Application Support/dev.tmgr.desktop/widget/timer.json"
        guard let data = FileManager.default.contents(atPath: path),
              let payload = try? JSONDecoder().decode(TimerPayload.self, from: data),
              payload.version == 1
        else { return nil }
        return payload
    }
}

struct TimerEntry: TimelineEntry {
    let date: Date
    let payload: TimerPayload?

    static let sample = TimerEntry(
        date: Date(),
        payload: TimerPayload(
            version: 1,
            updatedAt: Date().timeIntervalSince1970,
            total: 2,
            timers: [
                TimerItem(title: "Write release notes", startedAt: Date().timeIntervalSince1970 - 3725),
                TimerItem(title: "Review PR", startedAt: Date().timeIntervalSince1970 - 600),
            ]
        )
    )
    static let empty = TimerEntry(date: Date(), payload: nil)
}

struct TimerProvider: TimelineProvider {
    func placeholder(in context: Context) -> TimerEntry { .sample }

    func getSnapshot(in context: Context, completion: @escaping (TimerEntry) -> Void) {
        completion(context.isPreview ? .sample : TimerEntry(date: Date(), payload: TimerStore.load()))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<TimerEntry>) -> Void) {
        let now = Date()
        let entry = TimerEntry(date: now, payload: TimerStore.load())
        completion(Timeline(entries: [entry], policy: .after(now.addingTimeInterval(15 * 60))))
    }
}

struct TimerWidgetView: View {
    let entry: TimerEntry
    @Environment(\.widgetFamily) private var family

    var body: some View {
        content
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .widgetBackground()
    }

    @ViewBuilder
    private var content: some View {
        if let payload = entry.payload, let first = payload.timers.first {
            VStack(alignment: .leading, spacing: 4) {
                Label("Timer", systemImage: "timer")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(first.title)
                    .font(family == .systemSmall ? .headline : .title3.weight(.semibold))
                    .lineLimit(family == .systemSmall ? 2 : 3)
                Spacer(minLength: 0)
                Text(timerInterval: Date(timeIntervalSince1970: first.startedAt)...Date.distantFuture, countsDown: false)
                    .font(.system(size: family == .systemSmall ? 26 : 34, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    .minimumScaleFactor(0.6)
                    .lineLimit(1)
                if payload.total > 1 {
                    Text("+\(payload.total - 1) more")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        } else {
            VStack(alignment: .leading, spacing: 4) {
                Label("Timer", systemImage: "timer")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer(minLength: 0)
                Text("No timer running")
                    .font(.headline)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

extension View {
    @ViewBuilder
    func widgetBackground() -> some View {
        if #available(macOS 14.0, *) {
            self.containerBackground(.fill.tertiary, for: .widget)
        } else {
            self.padding().background(Color(nsColor: .windowBackgroundColor))
        }
    }
}

struct TimerWidget: Widget {
    let kind = "dev.tmgr.desktop.widget.timer"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TimerProvider()) { entry in
            TimerWidgetView(entry: entry)
        }
        .configurationDisplayName("Timer")
        .description("Shows the task timer that is running now.")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

@main
struct TMGRWidgetBundle: WidgetBundle {
    var body: some Widget {
        TimerWidget()
    }
}

#if DEBUG && canImport(PreviewsMacros)
#Preview("Running", as: .systemSmall) {
    TimerWidget()
} timeline: {
    TimerEntry.sample
}

#Preview("Idle", as: .systemMedium) {
    TimerWidget()
} timeline: {
    TimerEntry.empty
}
#endif
