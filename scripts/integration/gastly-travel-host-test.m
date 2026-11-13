// Hidden AppKit host contract test. No windows are ordered in, no synthetic input,
// no microphone/provider access, and no reading or changing app/user preferences.
#import <AppKit/AppKit.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct { double x; double y; double scale; bool returning; } WICompanionPointer;
extern bool wi_companion_panel_create(void *window);
extern bool wi_companion_panel_recording(bool recording);
extern bool wi_companion_panel_pointer(WICompanionPointer *result, bool recording);
extern bool wi_companion_panel_ignore_mouse(bool ignore);

static void check(BOOL value, const char *message) {
    if (!value) { fprintf(stderr, "FAIL: %s\n", message); exit(1); }
}
static void settle(void) {
    NSDate *end = [NSDate dateWithTimeIntervalSinceNow:0.8];
    while (end.timeIntervalSinceNow > 0)
        [NSRunLoop.currentRunLoop runMode:NSDefaultRunLoopMode beforeDate:end];
}
static BOOL sameFrame(NSRect a, NSRect b) {
    return fabs(a.origin.x-b.origin.x) < 0.1 && fabs(a.origin.y-b.origin.y) < 0.1 &&
        NSEqualSizes(a.size, b.size);
}
static void verifyHome(NSPanel *panel, NSRect home) {
    WICompanionPointer pointer = {0};
    check(wi_companion_panel_pointer(&pointer, false), "native pointer snapshot");
    check(!pointer.returning, "return flag settles at arrival");
    check(sameFrame(panel.frame, home), "exact saved home restored");
    check(!panel.visible && !NSApp.active, "test stays invisible and non-activating");
    check(!panel.canBecomeKeyWindow && !panel.canBecomeMainWindow, "panel never takes focus");
}
int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited];
        NSScreen *screen = NSScreen.mainScreen;
        check(screen != nil, "display available");
        NSRect visible = screen.visibleFrame;
        NSRect home = NSMakeRect(NSMinX(visible)+130, NSMinY(visible)+210, 180, 120);
        NSWindow *owner = [[NSWindow alloc] initWithContentRect:home
            styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
        check(wi_companion_panel_create((__bridge void *)owner), "create real panel");
        NSPanel *panel = nil;
        for (NSWindow *window in NSApp.windows)
            if ([window isKindOfClass:NSPanel.class]) panel = (NSPanel *)window;
        check(panel != nil, "find own hidden panel");
        [panel setFrame:home display:NO];
        check(wi_companion_panel_ignore_mouse(false), "request character input");
        check(wi_companion_panel_recording(true), "start recording travel");
        settle();
        NSRect dock = home;
        dock.origin.x = NSMidX(screen.frame)-90;
        dock.origin.y = NSMinY(visible)+36-(120-52.5);
        // AppKit aligns window origins to backing pixels (including half-point targets).
        BOOL dockAligned = fabs(panel.frame.origin.x-dock.origin.x) <= 1.0/panel.backingScaleFactor &&
            fabs(panel.frame.origin.y-dock.origin.y) <= 1.0/panel.backingScaleFactor && NSEqualSizes(panel.frame.size, dock.size);
        if (!dockAligned)
            fprintf(stderr, "Dock geometry: expected %s, actual %s, visible %s\n", NSStringFromRect(dock).UTF8String, NSStringFromRect(panel.frame).UTF8String, NSStringFromRect(visible).UTF8String);
        check(dockAligned, "waveform core docks above usable bottom-center");
        check(!panel.ignoresMouseEvents, "requested pointer mode restored after travel");
        WICompanionPointer publicationGap = {0};
        check(wi_companion_panel_pointer(&publicationGap, false) && publicationGap.returning,
            "processing cannot spin before the UI-thread return dispatch");
        check(wi_companion_panel_recording(true), "duplicate recording publication");
        check(wi_companion_panel_recording(false), "start return");
        WICompanionPointer pointer = {0};
        check(wi_companion_panel_pointer(&pointer, false), "return snapshot");
        if (pointer.returning) {
            check(wi_companion_panel_ignore_mouse(false), "hover request during travel");
            check(panel.ignoresMouseEvents, "hover cannot intercept input during automatic travel");
        }
        settle();
        verifyHome(panel, home);
        check(wi_companion_panel_recording(false), "duplicate processing publication");
        verifyHome(panel, home);

        // Interrupt a return with a new recording: never recapture the dock or
        // an intermediate frame as the original home; ignore stale completions.
        check(wi_companion_panel_recording(true), "second recording");
        settle();
        check(wi_companion_panel_recording(false), "second return");
        check(wi_companion_panel_recording(true), "recording interrupts return");
        settle();
        check(wi_companion_panel_recording(false), "final return");
        settle();
        verifyHome(panel, home);

        // Moving the idle character defines the next recording's home.
        home.origin.x += 60;
        home.origin.y += 35;
        [panel setFrame:home display:NO];
        check(wi_companion_panel_recording(true), "record after idle drag");
        settle();
        check(wi_companion_panel_recording(false), "return after idle drag");
        settle();
        verifyHome(panel, home);
        puts("PASS: hidden native travel, bottom-center geometry, exact home, repeated phases, interrupted return, new home, click-through and non-activation contracts");
    }
    return 0;
}
