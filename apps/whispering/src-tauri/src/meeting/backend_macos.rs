//! Actual Mac backend for the meeting actor. Creation is explicit and must run
//! on the actor thread after user-approved permissions. No permission prompts
//! here: unattended development/CI must never open an input or capture a screen.

use super::audio::{AudioPacket, AudioSource};
use super::macos::{self, NativeAudioInput};
use super::storage::{MeetingManifest, MeetingMode};
use super::worker::{CapturedScreenshot, MeetingBackend, ScreenshotJob};
use std::io;
use std::sync::mpsc::TryRecvError;

pub struct MacMeetingBackend {
    microphone: NativeAudioInput,
    system: Option<NativeAudioInput>,
    epoch_ns: u64,
    stopped: bool,
}
impl MacMeetingBackend {
    pub fn new(manifest: &MeetingManifest) -> io::Result<Self> {
        if !macos::screen_supported() {
            return Err(io::Error::other(
                "Meeting capture requires macOS 14 or newer",
            ));
        }
        let system = if manifest.mode == MeetingMode::SystemAndMicrophone {
            let display = manifest
                .display_id
                .ok_or_else(|| io::Error::other("Choose a display for system audio capture"))?;
            Some(NativeAudioInput::system(display)?)
        } else {
            None
        };
        let microphone = NativeAudioInput::microphone(&manifest.microphone_id)?;
        let epoch_ns = macos::host_time_ns();
        // Both inputs are initialized with delivery disabled. A slow second
        // input cannot overflow the first input's bounded transport queue.
        microphone.enable()?;
        if let Some(system) = &system {
            system.enable()?;
        }
        Ok(Self {
            microphone,
            system,
            epoch_ns,
            stopped: false,
        })
    }
}
impl MeetingBackend for MacMeetingBackend {
    fn epoch_ns(&self) -> u64 {
        self.epoch_ns
    }
    fn now_ns(&self) -> u64 {
        macos::host_time_ns()
    }
    fn poll_audio(&mut self) -> io::Result<Vec<AudioPacket>> {
        let mut packets = Vec::new();
        for (source, input) in [
            (AudioSource::Microphone, Some(&self.microphone)),
            (AudioSource::System, self.system.as_ref()),
        ] {
            if let Some(input) = input {
                // Each native queue holds at most 64 packets; this finite drain
                // keeps the command loop responsive even under sustained audio.
                for _ in 0..64 {
                    match input.try_recv() {
                        Ok(packet) if packet.source == source => packets.push(packet),
                        Ok(_) => return Err(io::Error::other("Native audio source mismatch")),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            return Err(io::Error::other("Native audio transport disconnected"))
                        }
                    }
                }
            }
        }
        Ok(packets)
    }
    fn check_health(&self) -> io::Result<()> {
        self.microphone.check_health()?;
        if let Some(system) = &self.system {
            system.check_health()?;
        }
        Ok(())
    }
    fn stop(&mut self) -> io::Result<()> {
        if self.stopped {
            return Ok(());
        }
        self.stopped = true;
        let mut failures = Vec::new();
        if let Err(error) = self.microphone.stop() {
            failures.push(error.to_string());
        }
        if let Some(system) = &mut self.system {
            if let Err(error) = system.stop() {
                failures.push(error.to_string());
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(io::Error::other(failures.join("; ")))
        }
    }
    fn stopped_integrity(&self) -> io::Result<()> {
        let losses = self.microphone.dropped_packets()
            + self
                .system
                .as_ref()
                .map(|input| input.dropped_packets())
                .unwrap_or(0);
        if losses == 0 {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "{losses} audio packets were lost"
            )))
        }
    }
    fn screenshot_job(&self, display_id: u32) -> ScreenshotJob {
        Box::new(move || {
            let png = macos::screenshot(display_id)?;
            // Screenshot manager has delivered this image; use its completion
            // host time, not the time a potentially slow PNG disk write ends.
            Ok(CapturedScreenshot {
                png,
                host_time_ns: macos::host_time_ns(),
            })
        })
    }
}
