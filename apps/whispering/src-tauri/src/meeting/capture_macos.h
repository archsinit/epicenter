#pragma once
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// Buffers are valid only for the callback duration. Callback must not block.
typedef void (*WIMeetingAudioCallback)(void *context, const float *samples,
    size_t frames, uint32_t sample_rate, uint64_t host_time_ns);
typedef struct { void *data; size_t length; } WIMeetingBytes;

bool wi_meeting_screen_supported(void);
// Normal OS prompts, only to be called after the user's explicit action.
bool wi_meeting_request_screen_permission(void);
bool wi_meeting_request_microphone_permission(void);
WIMeetingBytes wi_meeting_sources(char *error, size_t capacity);
void *wi_meeting_system_start(uint32_t display_id, WIMeetingAudioCallback callback,
    void *context, char *error, size_t capacity);
void *wi_meeting_microphone_start(const char *device_id, WIMeetingAudioCallback callback,
    void *context, char *error, size_t capacity);
// Inputs start with callback delivery disabled. Enable only when both sources
// are ready and the worker has chosen their common audio epoch.
bool wi_meeting_audio_enable(void *handle, char *error, size_t capacity);
// Disables callbacks and drains the serial output queue even if stop times out.
bool wi_meeting_audio_stop(void *handle, char *error, size_t capacity);
bool wi_meeting_audio_error(void *handle, char *error, size_t capacity);
WIMeetingBytes wi_meeting_screenshot(uint32_t display_id, char *error, size_t capacity);
uint64_t wi_meeting_host_time_ns(void);
void wi_meeting_free(void *data);
