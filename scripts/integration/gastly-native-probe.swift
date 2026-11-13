import AppKit
import ApplicationServices
import Foundation

let bundleID = "com.archsinit.whispering-intel"
let args = CommandLine.arguments
func report(_ value: [String: Any]) {
    if let bytes = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]),
       let text = String(data: bytes, encoding: .utf8) { print(text); fflush(stdout) }
}
func windows(_ pid: pid_t) -> [[String: Any]] {
    let items = CGWindowListCopyWindowInfo(.optionAll, kCGNullWindowID) as? [[String: Any]] ?? []
    return items.filter { ($0[kCGWindowOwnerPID as String] as? Int) == Int(pid) }
}
func inspect() {
    guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: bundleID).first else {
        report(["forkRunning": false]); return
    }
    let rows = windows(app.processIdentifier).map { w -> [String: Any] in
        return ["id": w[kCGWindowNumber as String] ?? 0,
                "name": w[kCGWindowName as String] ?? "",
                "bounds": w[kCGWindowBounds as String] ?? [:],
                "layer": w[kCGWindowLayer as String] ?? 0,
                "onscreen": w[kCGWindowIsOnscreen as String] ?? false,
                "alpha": w[kCGWindowAlpha as String] ?? 0]
    }
    report(["forkRunning": true, "pid": Int(app.processIdentifier),
            "activationPolicy": app.activationPolicy.rawValue,
            "windows": rows,
            "screenCaptureAvailable": CGPreflightScreenCaptureAccess()])
}

final class Fixture: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    var button: NSButton!
    var count = 0
    func applicationDidFinishLaunching(_ notification: Notification) {
        guard args.count >= 6, let x = Double(args[2]), let y = Double(args[3]),
              let width = Double(args[4]), let height = Double(args[5]) else { exit(2) }
        let screenTop = NSScreen.screens.first?.frame.maxY ?? 0
        window = NSWindow(contentRect: NSRect(x:x, y:screenTop-y-height, width:width, height:height),
                          styleMask:[.titled, .closable], backing:.buffered, defer:false)
        window.title = "Gastly isolated click-through test"
        window.backgroundColor = NSColor(calibratedRed:0.08, green:0.06, blue:0.12, alpha:1)
        button = NSButton(title:"Isolated test target — clicks: 0", target:self, action:#selector(clicked))
        button.frame = NSRect(x:0, y:0, width:width, height:height)
        window.contentView?.addSubview(button)
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps:true)
        report(["fixturePID": Int(ProcessInfo.processInfo.processIdentifier),
                "windowNumber": window.windowNumber])
        // Test fixture never outlives a failed test session indefinitely.
        Timer.scheduledTimer(withTimeInterval:180, repeats:false) { _ in NSApp.terminate(nil) }
    }
    @objc func clicked() {
        count += 1
        button.title = "Isolated test target — clicks: \(count)"
        report(["fixtureClicks": count])
    }
}

let mode = args.count > 1 ? args[1] : "inspect"
if mode == "fixture" {
    let app = NSApplication.shared
    let fixture = Fixture()
    app.setActivationPolicy(.accessory)
    app.delegate = fixture
    app.run()
} else if mode == "move" || mode == "click" || mode == "drag" {
    guard args.count >= 4, let x=Double(args[2]), let y=Double(args[3]) else { exit(2) }
    let point = CGPoint(x:x,y:y)
    if mode == "click" || mode == "drag" {
        // Never inject clicks unless OUR fixture is foreground and covers the entire gesture.
        let dx = mode == "drag" && args.count >= 7 ? Double(args[5]) ?? 0 : 0
        let dy = mode == "drag" && args.count >= 7 ? Double(args[6]) ?? 0 : 0
        let destination = CGPoint(x:x+dx,y:y+dy)
        guard args.count >= 5, let pid=Int32(args[4]),
              NSWorkspace.shared.frontmostApplication?.processIdentifier == pid,
              NSRunningApplication(processIdentifier:pid)?.executableURL?.resolvingSymlinksInPath().path == URL(fileURLWithPath:args[0]).resolvingSymlinksInPath().path,
              windows(pid).contains(where: { item in
                  guard let bounds = item[kCGWindowBounds as String] as? [String:Any],
                        let rect = CGRect(dictionaryRepresentation:bounds as CFDictionary) else { return false }
                  return rect.contains(point) && rect.contains(destination)
              })
        else { report(["refused": "Own isolated fixture is not foreground or does not cover gesture"]); exit(3) }
        guard CGPreflightPostEventAccess() else { report(["refused":"Normal event-post permission unavailable"]); exit(4) }
        CGEvent(mouseEventSource:nil, mouseType:.mouseMoved, mouseCursorPosition:point, mouseButton:.left)?.post(tap:.cghidEventTap)
        Thread.sleep(forTimeInterval:0.2)
        CGEvent(mouseEventSource:nil, mouseType:.leftMouseDown, mouseCursorPosition:point, mouseButton:.left)?.post(tap:.cghidEventTap)
        Thread.sleep(forTimeInterval:0.15)
        if mode == "drag" {
            for step in 1...12 {
                let p = CGPoint(x:x+dx*Double(step)/12.0, y:y+dy*Double(step)/12.0)
                CGEvent(mouseEventSource:nil, mouseType:.leftMouseDragged, mouseCursorPosition:p, mouseButton:.left)?.post(tap:.cghidEventTap)
                Thread.sleep(forTimeInterval:0.04)
            }
        }
        CGEvent(mouseEventSource:nil, mouseType:.leftMouseUp, mouseCursorPosition:destination, mouseButton:.left)?.post(tap:.cghidEventTap)
    } else {
        CGEvent(mouseEventSource:nil, mouseType:.mouseMoved, mouseCursorPosition:point, mouseButton:.left)?.post(tap:.cghidEventTap)
    }
    report(["eventPosted":mode])
} else if mode == "context" {
    let pointer = CGEvent(source:nil)?.location ?? .zero
    report(["pointerX":pointer.x,"pointerY":pointer.y,
            "frontmostPID":Int(NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0),
            "accessibilityTrusted":AXIsProcessTrusted(),
            "eventPostingAvailable":CGPreflightPostEventAccess()])
} else if mode == "restore" {
    guard args.count >= 3, let pid=Int32(args[2]) else { exit(2) }
    NSRunningApplication(processIdentifier:pid)?.activate(options:[])
} else {
    inspect()
}
