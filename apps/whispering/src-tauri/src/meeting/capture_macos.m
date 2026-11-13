// Native capture only. No files, provider calls, video recording, permissions
// bypass, or capture at initialization. All blocking entry points require a
// dedicated worker thread, never AppKit's main thread or an audio callback.
#import <AppKit/AppKit.h>
#import <AVFoundation/AVFoundation.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import "capture_macos.h"
#include <math.h>
#include <stdlib.h>
#include <string.h>

static void WIError(char *buffer, size_t capacity, NSString *message) {
    if (buffer && capacity) snprintf(buffer, capacity, "%s", message.UTF8String ?: "Capture failed");
}
static BOOL WIWorker(char *error, size_t capacity) {
    if (NSThread.isMainThread) {
        WIError(error, capacity, @"Meeting capture must run on its worker thread");
        return NO;
    }
    return YES;
}
static BOOL WIWait(dispatch_semaphore_t done) {
    return dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 15 * NSEC_PER_SEC)) == 0;
}
static WIMeetingBytes WIBytes(NSData *data) {
    if (!data.length) return (WIMeetingBytes){NULL, 0};
    void *copy = malloc(data.length);
    if (!copy) return (WIMeetingBytes){NULL, 0};
    memcpy(copy, data.bytes, data.length);
    return (WIMeetingBytes){copy, data.length};
}

bool wi_meeting_screen_supported(void) {
    if (@available(macOS 14.0, *)) return true;
    return false;
}

bool wi_meeting_request_screen_permission(void) {
    if (@available(macOS 14.0, *)) return CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess();
    return false;
}

bool wi_meeting_request_microphone_permission(void) {
    @autoreleasepool {
        if (NSThread.isMainThread) return false;
        AVAuthorizationStatus status = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
        if (status == AVAuthorizationStatusAuthorized) return true;
        if (status != AVAuthorizationStatusNotDetermined) return false;
        dispatch_semaphore_t done = dispatch_semaphore_create(0);
        __block BOOL granted = NO;
        [AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio completionHandler:^(BOOL allowed) {
            granted = allowed;
            dispatch_semaphore_signal(done);
        }];
        return WIWait(done) && granted;
    }
}

uint64_t wi_meeting_host_time_ns(void) {
    CMTime now = CMClockGetTime(CMClockGetHostTimeClock());
    return (uint64_t)CMTimeConvertScale(now, 1000000000, kCMTimeRoundingMethod_Default).value;
}
void wi_meeting_free(void *data) { free(data); }

WIMeetingBytes wi_meeting_sources(char *error, size_t capacity) {
    @autoreleasepool {
        if (!WIWorker(error, capacity)) return (WIMeetingBytes){0};
        NSMutableArray *microphones = [NSMutableArray array];
        // Enumeration alone does not request access or open an audio input.
        NSArray<AVCaptureDeviceType> *deviceTypes;
        if (@available(macOS 14.0, *)) {
            deviceTypes = @[AVCaptureDeviceTypeMicrophone, AVCaptureDeviceTypeExternal];
        } else {
            WIError(error, capacity, @"Meeting capture requires macOS 14 or newer");
            return (WIMeetingBytes){0};
        }
        AVCaptureDeviceDiscoverySession *discovery = [AVCaptureDeviceDiscoverySession
            discoverySessionWithDeviceTypes:deviceTypes
            mediaType:AVMediaTypeAudio position:AVCaptureDevicePositionUnspecified];
        for (AVCaptureDevice *device in discovery.devices) {
            [microphones addObject:@{@"id": device.uniqueID, @"name": device.localizedName}];
        }
        uint32_t count = 0;
        NSMutableArray *displays = [NSMutableArray array];
        if (CGGetOnlineDisplayList(0, NULL, &count) != kCGErrorSuccess || count > 64) {
            WIError(error, capacity, @"Could not enumerate displays");
            return (WIMeetingBytes){0};
        }
        CGDirectDisplayID ids[64];
        if (CGGetOnlineDisplayList(64, ids, &count) != kCGErrorSuccess) {
            WIError(error, capacity, @"Could not enumerate displays");
            return (WIMeetingBytes){0};
        }
        for (uint32_t i = 0; i < count; ++i) {
            [displays addObject:@{@"id": @(ids[i]), @"width": @(CGDisplayPixelsWide(ids[i])),
                @"height": @(CGDisplayPixelsHigh(ids[i])), @"main": @(ids[i] == CGMainDisplayID())}];
        }
        NSError *failure = nil;
        NSData *json = [NSJSONSerialization dataWithJSONObject:@{@"microphones": microphones,
            @"displays": displays, @"screenSupported": @(wi_meeting_screen_supported())}
            options:0 error:&failure];
        if (!json) WIError(error, capacity, failure.localizedDescription);
        WIMeetingBytes bytes = WIBytes(json);
        if (!bytes.data && json) WIError(error, capacity, @"Could not allocate source list");
        return bytes;
    }
}

API_AVAILABLE(macos(14.0))
static SCDisplay *WIDisplay(uint32_t identifier, char *error, size_t capacity) {
    if (!CGPreflightScreenCaptureAccess()) {
        WIError(error, capacity, @"Screen Recording permission is required. Enable it for Whispering Intel in macOS settings.");
        return nil;
    }
    dispatch_semaphore_t done = dispatch_semaphore_create(0);
    __block SCShareableContent *content = nil;
    __block NSError *failure = nil;
    [SCShareableContent getShareableContentExcludingDesktopWindows:YES onScreenWindowsOnly:YES
        completionHandler:^(SCShareableContent *result, NSError *problem) {
            content = result; failure = problem; dispatch_semaphore_signal(done);
        }];
    if (!WIWait(done)) { WIError(error, capacity, @"Timed out finding the selected display"); return nil; }
    if (!content) { WIError(error, capacity, failure.localizedDescription ?: @"Display enumeration failed"); return nil; }
    for (SCDisplay *display in content.displays) if (display.displayID == identifier) return display;
    WIError(error, capacity, @"The selected display is no longer available");
    return nil;
}

// One owner per input, one serial callback queue, mono float PCM at 48kHz.
// No video output is attached to SCStream and no video frames are retained.
API_AVAILABLE(macos(14.0))
@interface WIMeetingInput : NSObject <SCStreamOutput, SCStreamDelegate, AVCaptureAudioDataOutputSampleBufferDelegate>
@property(nonatomic, strong) SCStream *stream;
@property(nonatomic, strong) AVCaptureSession *microphone;
@property(nonatomic, strong) AVCaptureAudioDataOutput *microphoneOutput;
@property(nonatomic, strong) dispatch_queue_t queue;
@property(nonatomic) WIMeetingAudioCallback callback;
@property(nonatomic) void *context;
@property(atomic) BOOL enabled;
@property(atomic) BOOL cancelled;
@property(atomic, copy) NSString *failure;
- (void)consume:(CMSampleBufferRef)sample;
@end

@implementation WIMeetingInput
- (void)stream:(SCStream *)stream didStopWithError:(NSError *)error {
    self.failure = error.localizedDescription ?: @"System audio capture stopped";
}
- (void)stream:(SCStream *)stream didOutputSampleBuffer:(CMSampleBufferRef)sample ofType:(SCStreamOutputType)type {
    if (type == SCStreamOutputTypeAudio) [self consume:sample];
}
- (void)captureOutput:(AVCaptureOutput *)output didOutputSampleBuffer:(CMSampleBufferRef)sample fromConnection:(AVCaptureConnection *)connection {
    [self consume:sample];
}
- (void)consume:(CMSampleBufferRef)sample {
    @autoreleasepool {
        if (!self.enabled || !self.callback || !CMSampleBufferDataIsReady(sample)) return;
        CMAudioFormatDescriptionRef format = CMSampleBufferGetFormatDescription(sample);
        const AudioStreamBasicDescription *asbd = format ? CMAudioFormatDescriptionGetStreamBasicDescription(format) : NULL;
        CMItemCount frames = CMSampleBufferGetNumSamples(sample);
        if (!asbd || asbd->mFormatID != kAudioFormatLinearPCM ||
            !(asbd->mFormatFlags & kAudioFormatFlagIsFloat) ||
            (asbd->mFormatFlags & kAudioFormatFlagIsBigEndian) ||
            asbd->mChannelsPerFrame != 1 || asbd->mBitsPerChannel != 32 ||
            asbd->mBytesPerFrame != sizeof(float) || asbd->mSampleRate != 48000 ||
            frames <= 0 || frames > 96000) {
            self.failure = @"Capture returned an unsupported PCM audio format";
            return;
        }
        AudioBufferList list;
        CMBlockBufferRef retained = NULL;
        OSStatus status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(sample, NULL,
            &list, sizeof(list), kCFAllocatorDefault, kCFAllocatorDefault,
            kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment, &retained);
        CMTime pts = CMSampleBufferGetPresentationTimeStamp(sample);
        CMClockRef clock = self.microphone ? self.microphone.synchronizationClock
            : (self.stream ? self.stream.synchronizationClock : CMClockGetHostTimeClock());
        if (clock && CMTIME_IS_NUMERIC(pts)) pts = CMSyncConvertTime(pts, clock, CMClockGetHostTimeClock());
        if (status != noErr || list.mNumberBuffers != 1 || !list.mBuffers[0].mData ||
            list.mBuffers[0].mDataByteSize < (size_t)frames * sizeof(float) ||
            !clock || !CMTIME_IS_NUMERIC(pts) || pts.value < 0) {
            self.failure = @"Could not read captured PCM audio or its timestamp";
        } else {
            uint64_t ns = (uint64_t)CMTimeConvertScale(pts, 1000000000, kCMTimeRoundingMethod_Default).value;
            self.callback(self.context, list.mBuffers[0].mData, (size_t)frames, 48000, ns);
        }
        if (retained) CFRelease(retained);
    }
}
@end

API_AVAILABLE(macos(14.0))
static WIMeetingInput *WINewInput(WIMeetingAudioCallback callback, void *context) {
    if (!callback || !context) return nil;
    WIMeetingInput *input = [WIMeetingInput new];
    input.queue = dispatch_queue_create("com.archsinit.whispering-intel.meeting-audio", DISPATCH_QUEUE_SERIAL);
    input.callback = callback;
    input.context = context;
    return input;
}

void *wi_meeting_system_start(uint32_t displayID, WIMeetingAudioCallback callback,
    void *context, char *error, size_t capacity) {
    @autoreleasepool {
        if (!WIWorker(error, capacity)) return NULL;
        if (@available(macOS 14.0, *)) {
            SCDisplay *display = WIDisplay(displayID, error, capacity);
            if (!display) return NULL;
            WIMeetingInput *input = WINewInput(callback, context);
            if (!input) { WIError(error, capacity, @"Invalid audio callback"); return NULL; }
            SCContentFilter *filter = [[SCContentFilter alloc] initWithDisplay:display excludingWindows:@[]];
            SCStreamConfiguration *config = [SCStreamConfiguration new];
            config.width = 2; config.height = 2;
            config.minimumFrameInterval = CMTimeMake(1, 1);
            config.showsCursor = NO;
            config.capturesAudio = YES;
            config.sampleRate = 48000; config.channelCount = 1;
            config.excludesCurrentProcessAudio = YES;
            input.stream = [[SCStream alloc] initWithFilter:filter configuration:config delegate:input];
            NSError *failure = nil;
            if (![input.stream addStreamOutput:input type:SCStreamOutputTypeAudio sampleHandlerQueue:input.queue error:&failure]) {
                WIError(error, capacity, failure.localizedDescription); return NULL;
            }
            dispatch_semaphore_t done = dispatch_semaphore_create(0);
            __block NSError *startFailure = nil;
            [input.stream startCaptureWithCompletionHandler:^(NSError *problem) {
                startFailure = problem;
                // A late start must not leave capture running after a timeout.
                if (input.cancelled) [input.stream stopCaptureWithCompletionHandler:nil];
                dispatch_semaphore_signal(done);
            }];
            BOOL completed = WIWait(done);
            if (!completed || startFailure) {
                input.cancelled = YES;
                [input.stream stopCaptureWithCompletionHandler:nil];
                [input.stream removeStreamOutput:input type:SCStreamOutputTypeAudio error:nil];
                WIError(error, capacity, completed ? startFailure.localizedDescription : @"Timed out starting system audio");
                return NULL; // enabled was never set; context was never invoked.
            }
            // Enable callback delivery only after both sources and epoch are ready.
            return (void *)CFBridgingRetain(input);
        }
        WIError(error, capacity, @"Meeting capture requires macOS 14 or newer");
        return NULL;
    }
}

void *wi_meeting_microphone_start(const char *deviceID, WIMeetingAudioCallback callback,
    void *context, char *error, size_t capacity) {
    @autoreleasepool {
        if (!WIWorker(error, capacity)) return NULL;
        if (@available(macOS 14.0, *)) {
            if ([AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio] != AVAuthorizationStatusAuthorized) {
                WIError(error, capacity, @"Microphone permission is required for Whispering Intel"); return NULL;
            }
            NSString *identifier = deviceID ? [NSString stringWithUTF8String:deviceID] : @"";
            AVCaptureDevice *device = identifier.length ? [AVCaptureDevice deviceWithUniqueID:identifier]
                : [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeAudio];
            if (!device || ![device hasMediaType:AVMediaTypeAudio]) {
                WIError(error, capacity, @"The selected microphone is not available"); return NULL;
            }
            NSError *failure = nil;
            AVCaptureDeviceInput *source = [AVCaptureDeviceInput deviceInputWithDevice:device error:&failure];
            if (!source) { WIError(error, capacity, failure.localizedDescription); return NULL; }
            WIMeetingInput *input = WINewInput(callback, context);
            if (!input) { WIError(error, capacity, @"Invalid audio callback"); return NULL; }
            input.microphone = [AVCaptureSession new];
            input.microphoneOutput = [AVCaptureAudioDataOutput new];
            input.microphoneOutput.audioSettings = @{
                AVFormatIDKey: @(kAudioFormatLinearPCM), AVSampleRateKey: @48000,
                AVNumberOfChannelsKey: @1, AVLinearPCMBitDepthKey: @32,
                AVLinearPCMIsFloatKey: @YES, AVLinearPCMIsNonInterleaved: @NO
            };
            [input.microphoneOutput setSampleBufferDelegate:input queue:input.queue];
            if (![input.microphone canAddInput:source] || ![input.microphone canAddOutput:input.microphoneOutput]) {
                [input.microphoneOutput setSampleBufferDelegate:nil queue:NULL];
                WIError(error, capacity, @"Could not configure the microphone input"); return NULL;
            }
            [input.microphone beginConfiguration];
            [input.microphone addInput:source];
            [input.microphone addOutput:input.microphoneOutput];
            [input.microphone commitConfiguration];
            // AVCapture timestamps are converted to the same host clock used by
            // ScreenCaptureKit, allowing the worker to align separate sources.
            [input.microphoneOutput connectionWithMediaType:AVMediaTypeAudio].enabled = YES;
            [input.microphone startRunning];
            if (!input.microphone.running) {
                [input.microphoneOutput setSampleBufferDelegate:nil queue:NULL];
                WIError(error, capacity, @"Microphone capture did not start"); return NULL;
            }
            // Enable callback delivery only after both sources and epoch are ready.
            return (void *)CFBridgingRetain(input);
        }
        WIError(error, capacity, @"Meeting capture requires macOS 14 or newer");
        return NULL;
    }
}

bool wi_meeting_audio_enable(void *handle, char *error, size_t capacity) {
    @autoreleasepool {
        if (!WIWorker(error, capacity) || !handle) return false;
        if (@available(macOS 14.0, *)) {
            WIMeetingInput *input = (__bridge WIMeetingInput *)handle;
            if (input.cancelled || input.failure || (input.microphone && !input.microphone.running)) {
                WIError(error, capacity, input.failure ?: @"Audio input is not ready"); return false;
            }
            input.enabled = YES;
            return true;
        }
        WIError(error, capacity, @"Meeting capture requires macOS 14 or newer");
        return false;
    }
}

bool wi_meeting_audio_stop(void *handle, char *error, size_t capacity) {
    @autoreleasepool {
        if (!WIWorker(error, capacity) || !handle) return false;
        if (@available(macOS 14.0, *)) {
            WIMeetingInput *input = CFBridgingRelease(handle);
            input.enabled = NO;
            input.cancelled = YES;
            BOOL success = YES;
            if (input.stream) {
                dispatch_semaphore_t done = dispatch_semaphore_create(0);
                __block NSError *failure = nil;
                [input.stream stopCaptureWithCompletionHandler:^(NSError *problem) {
                    failure = problem; dispatch_semaphore_signal(done);
                }];
                success = WIWait(done);
                if (!success || failure) {
                    WIError(error, capacity, success ? failure.localizedDescription : @"Timed out stopping system audio");
                    success = NO;
                }
                [input.stream removeStreamOutput:input type:SCStreamOutputTypeAudio error:nil];
            }
            if (input.microphone) {
                [input.microphone stopRunning];
                [input.microphoneOutput setSampleBufferDelegate:nil queue:NULL];
            }
            dispatch_sync(input.queue, ^{}); // no context callbacks survive return
            input.callback = NULL; input.context = NULL;
            return success;
        }
        return false;
    }
}

bool wi_meeting_audio_error(void *handle, char *error, size_t capacity) {
    @autoreleasepool {
        if (@available(macOS 14.0, *)) {
            if (!handle) return false;
            WIMeetingInput *input = (__bridge WIMeetingInput *)handle;
            NSString *failure = input.failure;
            if (!failure && input.microphone && !input.microphone.running) failure = @"Microphone capture stopped unexpectedly";
            if (failure) { WIError(error, capacity, failure); return true; }
        }
        return false;
    }
}

WIMeetingBytes wi_meeting_screenshot(uint32_t displayID, char *error, size_t capacity) {
    @autoreleasepool {
        if (!WIWorker(error, capacity)) return (WIMeetingBytes){0};
        if (@available(macOS 14.0, *)) {
            SCDisplay *display = WIDisplay(displayID, error, capacity);
            if (!display) return (WIMeetingBytes){0};
            SCContentFilter *filter = [[SCContentFilter alloc] initWithDisplay:display excludingWindows:@[]];
            SCStreamConfiguration *config = [SCStreamConfiguration new];
            config.width = display.width; config.height = display.height;
            config.showsCursor = NO; config.capturesAudio = NO;
            dispatch_semaphore_t done = dispatch_semaphore_create(0);
            __block NSData *png = nil;
            __block NSError *failure = nil;
            [SCScreenshotManager captureImageWithFilter:filter configuration:config
                completionHandler:^(CGImageRef image, NSError *problem) {
                    if (image) png = [[[NSBitmapImageRep alloc] initWithCGImage:image]
                        representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
                    failure = problem;
                    dispatch_semaphore_signal(done);
                }];
            if (!WIWait(done)) { WIError(error, capacity, @"Timed out capturing screenshot"); return (WIMeetingBytes){0}; }
            if (!png) { WIError(error, capacity, failure.localizedDescription ?: @"Could not encode screenshot"); return (WIMeetingBytes){0}; }
            WIMeetingBytes bytes = WIBytes(png);
            if (!bytes.data) WIError(error, capacity, @"Could not allocate screenshot");
            return bytes;
        }
        WIError(error, capacity, @"Meeting screenshots require macOS 14 or newer");
        return (WIMeetingBytes){0};
    }
}
