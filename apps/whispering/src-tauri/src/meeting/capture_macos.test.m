// Synthetic-only fixture. Never enumerates sources, requests permissions,
// creates SCStream/AVCaptureSession, captures a screen or opens a microphone.
#import "capture_macos.m"
#include <assert.h>

static size_t calls;
static uint64_t lastTime;
static void packet(void *context, const float *samples, size_t frames, uint32_t rate, uint64_t time) {
    assert(context == &calls);
    assert(frames == 4 && rate == 48000);
    assert(samples[0] == 0.25f && samples[3] == -0.5f);
    lastTime = time;
    ++calls;
}
static CMSampleBufferRef sample(uint32_t channels) {
    AudioStreamBasicDescription asbd = {0};
    asbd.mSampleRate = 48000;
    asbd.mFormatID = kAudioFormatLinearPCM;
    asbd.mFormatFlags = kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked;
    asbd.mBytesPerPacket = asbd.mBytesPerFrame = sizeof(float) * channels;
    asbd.mFramesPerPacket = 1;
    asbd.mChannelsPerFrame = channels;
    asbd.mBitsPerChannel = 32;
    CMAudioFormatDescriptionRef format = NULL;
    assert(CMAudioFormatDescriptionCreate(kCFAllocatorDefault, &asbd, 0, NULL, 0, NULL, NULL, &format) == noErr);
    float data[8] = {0.25f, 0, 0, -0.5f, 0, 0, 0, 0};
    size_t size = 4 * asbd.mBytesPerFrame;
    CMBlockBufferRef block = NULL;
    assert(CMBlockBufferCreateWithMemoryBlock(kCFAllocatorDefault, NULL, size, kCFAllocatorDefault,
        NULL, 0, size, 0, &block) == noErr);
    assert(CMBlockBufferReplaceDataBytes(data, block, 0, size) == noErr);
    CMSampleTimingInfo timing = {CMTimeMake(1, 48000), CMTimeMake(100, 1), kCMTimeInvalid};
    CMSampleBufferRef buffer = NULL;
    assert(CMSampleBufferCreateReady(kCFAllocatorDefault, block, format, 4, 1, &timing,
        0, NULL, &buffer) == noErr);
    CFRelease(block); CFRelease(format);
    return buffer;
}
int main(void) {
    @autoreleasepool {
        assert(NSThread.isMainThread);
        char error[512] = {0};
        assert(!wi_meeting_system_start(0, packet, &calls, error, sizeof(error)));
        assert(strstr(error, "worker thread"));
        error[0] = 0;
        assert(!wi_meeting_microphone_start("", packet, &calls, error, sizeof(error)));
        assert(strstr(error, "worker thread"));
        error[0] = 0;
        assert(!wi_meeting_screenshot(0, error, sizeof(error)).data);
        assert(strstr(error, "worker thread"));
        assert(!wi_meeting_request_microphone_permission()); // main-thread guard
        assert(wi_meeting_host_time_ns() > 0);
        if (@available(macOS 14.0, *)) {
            WIMeetingInput *input = WINewInput(packet, &calls);
            CMSampleBufferRef buffer = sample(1);
            [input consume:buffer];
            assert(calls == 0); // no capture before explicit enable
            input.enabled = YES;
            [input consume:buffer];
            assert(calls == 1 && lastTime == 100000000000ULL);
            input.enabled = NO;
            [input consume:buffer];
            assert(calls == 1); // stopped/disabled input cannot call context
            input.enabled = YES;
            CMSampleBufferRef wrong = sample(2);
            [input consume:wrong];
            assert(calls == 1 && input.failure.length > 0);
            CFRelease(wrong); CFRelease(buffer);
            dispatch_semaphore_t entered = dispatch_semaphore_create(0);
            dispatch_semaphore_t releaseQueue = dispatch_semaphore_create(0);
            dispatch_async(input.queue, ^{
                dispatch_semaphore_signal(entered);
                assert(WIWait(releaseQueue));
            });
            assert(WIWait(entered));
            dispatch_semaphore_t done = dispatch_semaphore_create(0);
            void *handle = (void *)CFBridgingRetain(input);
            dispatch_async(dispatch_get_global_queue(QOS_CLASS_DEFAULT, 0), ^{
                char stopError[512] = {0};
                assert(wi_meeting_audio_stop(handle, stopError, sizeof(stopError)));
                dispatch_semaphore_signal(done);
            });
            // Stop must first disable callbacks, then wait for the queue. The
            // fixture controls that queue without any real capture session.
            for (int i = 0; i < 1000 && input.enabled; ++i) [NSThread sleepForTimeInterval:0.001];
            assert(!input.enabled);
            assert(dispatch_semaphore_wait(done, DISPATCH_TIME_NOW) != 0);
            dispatch_semaphore_signal(releaseQueue);
            assert(WIWait(done));
            assert(!input.enabled && !input.callback && !input.context);
        }
        puts("PASS: synthetic native PCM forwarding, timestamps, format rejection, disabled/stop guards; no live capture");
    }
    return 0;
}
