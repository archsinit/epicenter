// Public AppKit host for a non-activating companion. No class swizzling, event taps,
// focus restoration tricks, or changes to the user's OS permissions.
#import <AppKit/AppKit.h>
#include <stdbool.h>

typedef struct { double x; double y; double scale; } WICompanionPointer;

@interface WICompanionPanel : NSPanel
@end
@implementation WICompanionPanel
- (BOOL)canBecomeKeyWindow { return NO; }
- (BOOL)canBecomeMainWindow { return NO; }
@end

// A native transparent drag surface avoids making WKWebView a key responder.
// Its hit-test matches the frontend/native ellipse, not the rectangular window.
@interface WICompanionDragView : NSView
@property(nonatomic) BOOL wide;
@end
@implementation WICompanionDragView
- (BOOL)isOpaque { return NO; }
- (BOOL)acceptsFirstMouse:(NSEvent *)event { return YES; }
- (BOOL)needsPanelToBecomeKey { return NO; }
- (NSView *)hitTest:(NSPoint)point {
    const double rx = self.wide ? 82.0 : 55.0;
    const double ry = self.wide ? 24.0 : 50.0;
    const double x = (point.x - self.bounds.size.width / 2.0) / rx;
    const double y = (self.bounds.size.height - point.y - 52.5) / ry;
    return x * x + y * y <= 1.0 ? self : nil;
}
- (void)mouseDown:(NSEvent *)event { [self.window performWindowDragWithEvent:event]; }
@end

static WICompanionPanel *companionPanel;
static WICompanionDragView *dragSurface;

bool wi_companion_panel_create(void *windowPointer) {
    if (![NSThread isMainThread] || companionPanel != nil || windowPointer == NULL) return false;
    NSWindow *original = (__bridge NSWindow *)windowPointer;
    NSView *content = original.contentView; // ARC retains during the transfer.
    if (content == nil) return false;
    NSRect rect = [original contentRectForFrameRect:original.frame];
    WICompanionPanel *panel = [[WICompanionPanel alloc]
        initWithContentRect:rect
        styleMask:NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel
        backing:NSBackingStoreBuffered defer:NO];
    panel.title = @"Recording status";
    panel.level = original.level;
    panel.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorFullScreenAuxiliary;
    panel.hasShadow = NO;
    panel.opaque = NO;
    panel.backgroundColor = NSColor.clearColor;
    panel.hidesOnDeactivate = NO;
    panel.becomesKeyOnlyIfNeeded = YES;
    panel.releasedWhenClosed = NO;
    panel.ignoresMouseEvents = YES;
    panel.acceptsMouseMovedEvents = YES;
    // Keep Tauri's original window alive as the webview/IPC owner, but never show it.
    // The real NSPanel owns the existing content view; no object changes its class.
    original.contentView = [[NSView alloc] initWithFrame:content.frame];
    NSView *container = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, rect.size.width, rect.size.height)];
    content.frame = container.bounds;
    content.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    [container addSubview:content];
    dragSurface = [[WICompanionDragView alloc] initWithFrame:container.bounds];
    dragSurface.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    [container addSubview:dragSurface positioned:NSWindowAbove relativeTo:content];
    panel.contentView = container;
    companionPanel = panel;
    return true;
}

bool wi_companion_panel_show(void) {
    if (![NSThread isMainThread] || companionPanel == nil) return false;
    [companionPanel orderFrontRegardless];
    return true;
}

bool wi_companion_panel_pointer(WICompanionPointer *result, bool recording) {
    if (![NSThread isMainThread] || companionPanel == nil || result == NULL) return false;
    dragSurface.wide = recording;
    NSPoint local = [companionPanel convertPointFromScreen:NSEvent.mouseLocation];
    result->x = local.x;
    result->y = companionPanel.contentView.bounds.size.height - local.y;
    result->scale = companionPanel.backingScaleFactor;
    return true;
}

bool wi_companion_panel_ignore_mouse(bool ignore) {
    if (![NSThread isMainThread] || companionPanel == nil) return false;
    companionPanel.ignoresMouseEvents = ignore;
    return true;
}

bool wi_companion_panel_drag(void) {
    if (![NSThread isMainThread] || companionPanel == nil) return false;
    NSEvent *event = NSApp.currentEvent;
    if (event == nil || event.type != NSEventTypeLeftMouseDown) {
        event = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDown
            location:[companionPanel convertPointFromScreen:NSEvent.mouseLocation]
            modifierFlags:event.modifierFlags timestamp:NSProcessInfo.processInfo.systemUptime
            windowNumber:companionPanel.windowNumber context:nil eventNumber:0 clickCount:1 pressure:1.0];
    }
    [companionPanel performWindowDragWithEvent:event];
    return true;
}
