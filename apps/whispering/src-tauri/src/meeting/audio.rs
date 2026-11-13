//! Bounded transport from real-time native callbacks to the meeting worker.
//! Never block a capture callback on disk I/O or a full queue. An overflow is
//! counted explicitly so the controller can report an interrupted recording.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
pub const MAX_PACKET_FRAMES: usize = 96_000;
const QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioSource {
    Microphone,
    System,
}

#[derive(Debug)]
pub struct AudioPacket {
    pub source: AudioSource,
    /// Monotonic host-clock time of the first frame, shared by both inputs.
    pub host_time_ns: u64,
    /// Mono 48kHz float PCM, independent of the native sample buffer lifetime.
    pub samples: Vec<f32>,
}

pub(crate) struct AudioSink {
    source: AudioSource,
    sender: SyncSender<AudioPacket>,
    dropped: AtomicU64,
}

impl AudioSink {
    pub(crate) fn channel(source: AudioSource) -> (Box<Self>, Receiver<AudioPacket>) {
        let (sender, receiver) = sync_channel(QUEUE_CAPACITY);
        (
            Box::new(Self {
                source,
                sender,
                dropped: AtomicU64::new(0),
            }),
            receiver,
        )
    }

    pub(crate) fn deliver(&self, samples: &[f32], sample_rate: u32, host_time_ns: u64) {
        if samples.is_empty()
            || samples.len() > MAX_PACKET_FRAMES
            || sample_rate != AUDIO_SAMPLE_RATE
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let packet = AudioPacket {
            source: self.source,
            host_time_ns,
            samples: samples
                .iter()
                .map(|sample| {
                    if sample.is_finite() {
                        sample.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    }
                })
                .collect(),
        };
        if self.sender.try_send(packet).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_own_samples_and_preserve_source_and_shared_timestamp() {
        let (sink, receiver) = AudioSink::channel(AudioSource::System);
        let mut samples = [0.25, -0.5];
        sink.deliver(&samples, AUDIO_SAMPLE_RATE, 123_456);
        samples[0] = 1.0;
        assert_eq!(samples[0], 1.0);
        let packet = receiver.try_recv().unwrap();
        assert_eq!(packet.samples, [0.25, -0.5]);
        assert_eq!(packet.source, AudioSource::System);
        assert_eq!(packet.host_time_ns, 123_456);
        assert_eq!(sink.dropped(), 0);
    }

    #[test]
    fn full_queue_drops_without_blocking_and_exposes_loss() {
        let (sink, receiver) = AudioSink::channel(AudioSource::Microphone);
        for i in 0..QUEUE_CAPACITY + 3 {
            sink.deliver(&[0.0], AUDIO_SAMPLE_RATE, i as u64);
        }
        assert_eq!(sink.dropped(), 3);
        assert_eq!(receiver.try_iter().count(), QUEUE_CAPACITY);
    }

    #[test]
    fn disconnected_consumer_is_reported_as_loss() {
        let (sink, receiver) = AudioSink::channel(AudioSource::Microphone);
        drop(receiver);
        sink.deliver(&[0.0], AUDIO_SAMPLE_RATE, 1);
        assert_eq!(sink.dropped(), 1);
    }

    #[test]
    fn invalid_rate_empty_or_oversized_packet_is_rejected() {
        let (sink, receiver) = AudioSink::channel(AudioSource::Microphone);
        sink.deliver(&[], AUDIO_SAMPLE_RATE, 1);
        sink.deliver(&[0.0], 44_100, 1);
        sink.deliver(&vec![0.0; MAX_PACKET_FRAMES + 1], AUDIO_SAMPLE_RATE, 1);
        assert_eq!(sink.dropped(), 3);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn pcm_is_finite_and_bounded_before_storage_or_mixing() {
        let (sink, receiver) = AudioSink::channel(AudioSource::Microphone);
        sink.deliver(&[f32::NAN, f32::INFINITY, -2.0, 3.0], AUDIO_SAMPLE_RATE, 1);
        assert_eq!(receiver.try_recv().unwrap().samples, [0.0, 0.0, -1.0, 1.0]);
    }
}
