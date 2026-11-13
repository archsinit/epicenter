// Public AppKit host for a non-activating companion. No class swizzling, event taps,
// focus restoration tricks, or changes to the user's OS permissions.
#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#include <stdbool.h>

typedef struct { double x; double y; double scale; bool returning; } WICompanionPointer;
static BOOL moving = NO;
static BOOL requestedIgnore = YES;
static BOOL recordingDocked = NO;
static BOOL returningHome = NO;
static BOOL hasHome = NO;
static NSRect homeFrame;
static NSScreen *homeScreen;
static NSUInteger moveGeneration = 0;

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
    if (moving) return nil;
    const double rx = self.wide ? 82.0 : 55.0;
    const double ry = self.wide ? 24.0 : 50.0;
    const double x = (point.x - self.bounds.size.width / 2.0) / rx;
    const double y = (self.bounds.size.height - point.y - 52.5) / ry;
    return x * x + y * y <= 1.0 ? self : nil;
}
- (void)mouseDown:(NSEvent *)event {
    if (!moving) [self.window performWindowDragWithEvent:event];
}
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

// Coordinates remain AppKit screen points throughout, including mixed-scale displays.
// Keep the waveform's core (52.5pt from the top) 36pt above the usable bottom edge.
bool wi_companion_panel_recording(bool recording) {
    if (![NSThread isMainThread] || companionPanel == nil) return false;
    if (recording == recordingDocked) return true;
    if (recording && !hasHome) {
        homeFrame = companionPanel.frame;
        homeScreen = companionPanel.screen ?: NSScreen.mainScreen;
        hasHome = YES;
    }
    recordingDocked = recording;
    returningHome = !recording;
    NSRect destination = homeFrame;
    if (recording) {
        NSRect visible = (homeScreen ?: NSScreen.mainScreen).visibleFrame;
        destination.origin.x = NSMidX((homeScreen ?: NSScreen.mainScreen).frame) - destination.size.width / 2.0;
        destination.origin.y = NSMinY(visible) + 36.0 - (destination.size.height - 52.5);
    }
    const NSUInteger generation = ++moveGeneration;
    moving = YES;
    companionPanel.ignoresMouseEvents = YES;
    void (^finish)(void) = ^{
        if (generation != moveGeneration) return; // A new take interrupted this trip.
        moving = NO;
        returningHome = NO;
        if (!recordingDocked) {
            hasHome = NO;
            homeScreen = nil;
        }
        companionPanel.ignoresMouseEvents = requestedIgnore;
    };
    if (NSWorkspace.sharedWorkspace.accessibilityDisplayShouldReduceMotion) {
        [companionPanel setFrame:destination display:YES];
        finish();
    } else {
        [NSAnimationContext runAnimationGroup:^(NSAnimationContext *context) {
            context.duration = 0.55;
            context.timingFunction = [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionEaseInEaseOut];
            [[companionPanel animator] setFrame:destination display:YES];
        } completionHandler:finish];
    }
    return true;
}

bool wi_companion_panel_pointer(WICompanionPointer *result, bool recording) {
    if (![NSThread isMainThread] || companionPanel == nil || result == NULL) return false;
    dragSurface.wide = recording;
    NSPoint local = [companionPanel convertPointFromScreen:NSEvent.mouseLocation];
    result->x = local.x;
    result->y = companionPanel.contentView.bounds.size.height - local.y;
    result->scale = companionPanel.backingScaleFactor;
    // The pipeline phase may publish just before its UI-thread move is dispatched.
    result->returning = returningHome || (recordingDocked && !recording);
    return true;
}

bool wi_companion_panel_ignore_mouse(bool ignore) {
    if (![NSThread isMainThread] || companionPanel == nil) return false;
    requestedIgnore = ignore;
    companionPanel.ignoresMouseEvents = ignore || moving;
    return true;
}

bool wi_companion_panel_drag(void) {
    if (![NSThread isMainThread] || companionPanel == nil) return false;
    if (moving) return true; // Automatic travel is not a failed user drag.
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
