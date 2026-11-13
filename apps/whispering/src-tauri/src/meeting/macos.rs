//! Safe worker-thread ownership of the Objective-C capture bridge. This module
//! does not start capture on import; the meeting controller must explicitly
//! request permissions and construct each input after the user's Start action.

use super::audio::{AudioPacket, AudioSink, AudioSource, MAX_PACKET_FRAMES};
use serde::{Deserialize, Serialize};
use std::ffi::{c_char, c_void, CStr, CString};
use std::io;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError};

type AudioCallback = unsafe extern "C" fn(*mut c_void, *const f32, usize, u32, u64);
#[repr(C)]
struct NativeBytes {
    data: *mut c_void,
    length: usize,
}

extern "C" {
    fn wi_meeting_screen_supported() -> bool;
    fn wi_meeting_request_screen_permission() -> bool;
    fn wi_meeting_request_microphone_permission() -> bool;
    fn wi_meeting_sources(error: *mut c_char, capacity: usize) -> NativeBytes;
    fn wi_meeting_system_start(
        display_id: u32,
        callback: AudioCallback,
        context: *mut c_void,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn wi_meeting_microphone_start(
        device_id: *const c_char,
        callback: AudioCallback,
        context: *mut c_void,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn wi_meeting_audio_enable(handle: *mut c_void, error: *mut c_char, capacity: usize) -> bool;
    fn wi_meeting_audio_stop(handle: *mut c_void, error: *mut c_char, capacity: usize) -> bool;
    fn wi_meeting_audio_error(handle: *mut c_void, error: *mut c_char, capacity: usize) -> bool;
    fn wi_meeting_screenshot(display_id: u32, error: *mut c_char, capacity: usize) -> NativeBytes;
    fn wi_meeting_host_time_ns() -> u64;
    fn wi_meeting_free(data: *mut c_void);
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MicrophoneSource {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct DisplaySource {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub main: bool,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSources {
    pub microphones: Vec<MicrophoneSource>,
    pub displays: Vec<DisplaySource>,
    pub screen_supported: bool,
}

fn native_error(buffer: &[c_char]) -> io::Error {
    // Every native error buffer starts zeroed and native writes use snprintf.
    let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy();
    io::Error::other(if message.is_empty() {
        "Native capture failed".into()
    } else {
        message.into_owned()
    })
}
impl NativeBytes {
    fn as_slice(&self, limit: usize) -> io::Result<&[u8]> {
        if self.data.is_null() || self.length == 0 || self.length > limit {
            return Err(io::Error::other(
                "Native capture returned an invalid buffer",
            ));
        }
        // malloc-owned native bytes remain alive until this object's Drop.
        Ok(unsafe { std::slice::from_raw_parts(self.data.cast::<u8>(), self.length) })
    }
}
impl Drop for NativeBytes {
    fn drop(&mut self) {
        unsafe { wi_meeting_free(self.data) };
    }
}

pub fn screen_supported() -> bool {
    unsafe { wi_meeting_screen_supported() }
}
pub fn host_time_ns() -> u64 {
    unsafe { wi_meeting_host_time_ns() }
}
pub fn request_screen_permission() -> bool {
    unsafe { wi_meeting_request_screen_permission() }
}
pub fn request_microphone_permission() -> bool {
    unsafe { wi_meeting_request_microphone_permission() }
}

pub fn sources() -> io::Result<CaptureSources> {
    let mut error = [0; 1024];
    let bytes = unsafe { wi_meeting_sources(error.as_mut_ptr(), error.len()) };
    if bytes.data.is_null() {
        return Err(native_error(&error));
    }
    Ok(serde_json::from_slice(bytes.as_slice(4 * 1024 * 1024)?)?)
}

pub fn screenshot(display_id: u32) -> io::Result<Vec<u8>> {
    let mut error = [0; 1024];
    let bytes = unsafe { wi_meeting_screenshot(display_id, error.as_mut_ptr(), error.len()) };
    if bytes.data.is_null() {
        return Err(native_error(&error));
    }
    let png = bytes.as_slice(256 * 1024 * 1024)?;
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(io::Error::other("Native screenshot is not PNG data"));
    }
    Ok(png.to_vec())
}

unsafe extern "C" fn receive_audio(
    context: *mut c_void,
    samples: *const f32,
    frames: usize,
    sample_rate: u32,
    host_time_ns: u64,
) {
    if context.is_null() {
        return;
    }
    // Context is a stable Box, owned until native stop disables and drains all
    // callbacks. The native bridge guarantees samples live for this call only.
    let sink = unsafe { &*context.cast::<AudioSink>() };
    if samples.is_null() || frames > MAX_PACKET_FRAMES {
        sink.deliver(&[], sample_rate, host_time_ns);
        return;
    }
    sink.deliver(
        unsafe { std::slice::from_raw_parts(samples, frames) },
        sample_rate,
        host_time_ns,
    );
}

/// Deliberately !Send and !Sync: create, use, stop and drop on the same dedicated
/// meeting worker. The boxed callback context cannot outlive that input.
pub struct NativeAudioInput {
    handle: Option<NonNull<c_void>>,
    sink: Box<AudioSink>,
    receiver: Receiver<AudioPacket>,
    _worker_only: PhantomData<Rc<()>>,
}

impl NativeAudioInput {
    pub fn system(display_id: u32) -> io::Result<Self> {
        Self::start(AudioSource::System, |context, error| unsafe {
            wi_meeting_system_start(
                display_id,
                receive_audio,
                context,
                error.as_mut_ptr(),
                error.len(),
            )
        })
    }
    /// Empty device ID selects the current default; a missing explicit device
    /// fails instead of silently recording a different microphone.
    pub fn microphone(device_id: &str) -> io::Result<Self> {
        let identifier = CString::new(device_id)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid microphone ID"))?;
        Self::start(AudioSource::Microphone, |context, error| unsafe {
            wi_meeting_microphone_start(
                identifier.as_ptr(),
                receive_audio,
                context,
                error.as_mut_ptr(),
                error.len(),
            )
        })
    }
    fn start(
        source: AudioSource,
        create: impl FnOnce(*mut c_void, &mut [c_char]) -> *mut c_void,
    ) -> io::Result<Self> {
        let (mut sink, receiver) = AudioSink::channel(source);
        let mut error = [0; 1024];
        let handle = create((&mut *sink as *mut AudioSink).cast(), &mut error);
        let handle = NonNull::new(handle).ok_or_else(|| native_error(&error))?;
        Ok(Self {
            handle: Some(handle),
            sink,
            receiver,
            _worker_only: PhantomData,
        })
    }
    /// Start delivering PCM only once every source has been initialized. This
    /// avoids overflowing the first source's queue while the second is starting.
    pub fn enable(&self) -> io::Result<()> {
        let handle = self
            .handle
            .ok_or_else(|| io::Error::other("Audio input is stopped"))?;
        let mut error = [0; 1024];
        if !unsafe { wi_meeting_audio_enable(handle.as_ptr(), error.as_mut_ptr(), error.len()) } {
            return Err(native_error(&error));
        }
        Ok(())
    }

    pub fn try_recv(&self) -> Result<AudioPacket, TryRecvError> {
        self.receiver.try_recv()
    }
    pub fn dropped_packets(&self) -> u64 {
        self.sink.dropped()
    }
    pub fn check_health(&self) -> io::Result<()> {
        if self.dropped_packets() != 0 {
            return Err(io::Error::other(format!(
                "{} audio packets lost; meeting recording is incomplete",
                self.dropped_packets()
            )));
        }
        let handle = self
            .handle
            .ok_or_else(|| io::Error::other("Audio input is stopped"))?;
        let mut error = [0; 1024];
        if unsafe { wi_meeting_audio_error(handle.as_ptr(), error.as_mut_ptr(), error.len()) } {
            return Err(native_error(&error));
        }
        Ok(())
    }
    /// Queue contents can still be drained with try_recv after stopping. Native
    /// stop drains callbacks even on failure, so freeing the context is safe.
    pub fn stop(&mut self) -> io::Result<()> {
        if let Some(handle) = self.handle.take() {
            let mut error = [0; 1024];
            if !unsafe { wi_meeting_audio_stop(handle.as_ptr(), error.as_mut_ptr(), error.len()) } {
                return Err(native_error(&error));
            }
        }
        Ok(())
    }
}
impl Drop for NativeAudioInput {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("Meeting audio stop failed: {error}");
        }
    }
}
