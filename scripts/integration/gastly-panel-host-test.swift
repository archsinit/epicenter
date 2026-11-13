import AppKit
import Foundation
@_silgen_name("wi_companion_panel_create") func panelCreate(_ pointer:UnsafeMutableRawPointer)->Bool
@_silgen_name("wi_companion_panel_show") func panelShow()->Bool
@_silgen_name("wi_companion_panel_ignore_mouse") func ignoreMouse(_ ignore:Bool)->Bool
@_silgen_name("wi_companion_panel_drag") func panelDrag()->Bool
final class DragView:NSView {
 override var needsPanelToBecomeKey: Bool { false }
 override func acceptsFirstMouse(for event:NSEvent?)->Bool { true }
 override func mouseDown(with event:NSEvent) {
  print("DRAG_START");fflush(stdout)
  print("DRAG_RESULT \(panelDrag())");fflush(stdout)
 }
 override func draw(_ dirtyRect:NSRect){NSColor.systemPurple.setFill();bounds.fill()}
}
final class Delegate:NSObject,NSApplicationDelegate {
 var original:NSWindow!
 func applicationDidFinishLaunching(_ n:Notification){
  original=NSWindow(contentRect:NSRect(x:790,y:146,width:90,height:60),styleMask:[.borderless],backing:.buffered,defer:false)
  original.level = .floating;original.collectionBehavior = [.canJoinAllSpaces,.fullScreenAuxiliary];original.contentView=DragView(frame:NSRect(x:0,y:0,width:90,height:60))
  let ok=panelCreate(Unmanaged.passUnretained(original).toOpaque()) && ignoreMouse(false) && panelShow()
  print("HOST_READY \(ok)")
  for w in NSApp.windows { print("WINDOW \(w.frame) visible=\(w.isVisible) activeSpace=\(w.isOnActiveSpace) level=\(w.level.rawValue) ignored=\(w.ignoresMouseEvents) view=\(String(describing:w.contentView)) bounds=\(String(describing:w.contentView?.bounds))") }
  fflush(stdout)
  Timer.scheduledTimer(withTimeInterval:3,repeats:false){_ in
   for w in NSApp.windows where w is NSPanel { print("AFTER_DRAG \(w.frame)");fflush(stdout) }
  }
  Timer.scheduledTimer(withTimeInterval:40,repeats:false){_ in NSApp.terminate(nil)}
 }
}
let app=NSApplication.shared;let d=Delegate();app.setActivationPolicy(.accessory);app.delegate=d;app.run()
