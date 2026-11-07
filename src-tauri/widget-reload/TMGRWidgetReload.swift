import WidgetKit

@_cdecl("tmgr_widget_reload")
public func tmgrWidgetReload() {
  WidgetCenter.shared.reloadTimelines(ofKind: "dev.tmgr.desktop.widget.timer")
}
