//! Progressive, bounded-memory source WAVs and post-recording mixing. Source
//! chunks use a common one-minute grid. Quiet missing buckets are represented
//! by absent source files, not minutes of fabricated input packets. Mixing
//! supplies silence for those gaps and never reads screenshot assets.

use super::audio::{AudioPacket, AudioSource, AUDIO_SAMPLE_RATE, MAX_PACKET_FRAMES};
use super::storage::{AudioTrack, MeetingStore, PendingAudioChunk};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};

pub const AUDIO_CHUNK_MS: u64 = 60_000;
const CHUNK_FRAMES: u64 = AUDIO_SAMPLE_RATE as u64 * 60;

fn wav_error(error: hound::Error) -> io::Error {
    io::Error::other(error)
}
fn spec() -> hound::WavSpec {
    hound::WavSpec {
        channels: 1,
        sample_rate: AUDIO_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    }
}
fn to_pcm(sample: f32) -> i16 {
    if sample.is_finite() {
        (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
    } else {
        0
    }
}

struct ActiveChunk {
    pending: PendingAudioChunk,
    writer: hound::WavWriter<BufWriter<File>>,
    bucket: u64,
    frames: u64,
}
impl ActiveChunk {
    fn new(store: &MeetingStore, bucket: u64, track: AudioTrack) -> io::Result<Self> {
        let pending = store.begin_track_chunk(
            bucket
                .checked_mul(AUDIO_CHUNK_MS)
                .ok_or_else(|| io::Error::other("Audio timestamp overflow"))?,
            track,
        )?;
        let writer = hound::WavWriter::new(BufWriter::new(pending.writer_file()?), spec())
            .map_err(wav_error)?;
        Ok(Self {
            pending,
            writer,
            bucket,
            frames: 0,
        })
    }
    fn silence_until(&mut self, frames: u64) -> io::Result<()> {
        while self.frames < frames {
            self.writer.write_sample(0i16).map_err(wav_error)?;
            self.frames += 1;
        }
        Ok(())
    }
    fn checkpoint(&mut self) -> io::Result<()> {
        self.writer.flush().map_err(wav_error)?; // also refreshes WAV lengths
        self.pending.file_mut().sync_data()
    }
    fn finish(self, store: &mut MeetingStore) -> io::Result<()> {
        self.writer.finalize().map_err(wav_error)?;
        let duration_ms = (self.frames * 1_000).div_ceil(AUDIO_SAMPLE_RATE as u64);
        store.commit_audio_chunk(self.pending, duration_ms)
    }
}

pub struct SourceTimeline {
    source: AudioSource,
    track: AudioTrack,
    epoch_ns: u64,
    next_frame: u64,
    active: Option<ActiveChunk>,
}
impl SourceTimeline {
    pub fn new(source: AudioSource, epoch_ns: u64) -> Self {
        Self {
            source,
            epoch_ns,
            next_frame: 0,
            active: None,
            track: match source {
                AudioSource::Microphone => AudioTrack::Microphone,
                AudioSource::System => AudioTrack::System,
            },
        }
    }
    pub fn push(&mut self, packet: AudioPacket, store: &mut MeetingStore) -> io::Result<()> {
        if packet.source != self.source
            || packet.samples.is_empty()
            || packet.samples.len() > MAX_PACKET_FRAMES
        {
            return Err(io::Error::other("Invalid source audio packet"));
        }
        let frame = ((packet.host_time_ns as i128 - self.epoch_ns as i128)
            * AUDIO_SAMPLE_RATE as i128)
            .div_euclid(1_000_000_000);
        let end = frame + packet.samples.len() as i128;
        if end <= self.next_frame as i128 {
            return Ok(());
        } // old/overlapping packet
        let start = frame.max(self.next_frame as i128);
        if self.source == AudioSource::Microphone
            && start
                > self
                    .next_frame
                    .saturating_add(10 * AUDIO_SAMPLE_RATE as u64) as i128
        {
            return Err(io::Error::other(
                "Microphone audio has a gap longer than ten seconds",
            ));
        }
        let skip = (start - frame) as usize;
        let mut samples = &packet.samples[skip..];
        let mut frame = start as u64;
        while !samples.is_empty() {
            let bucket = frame / CHUNK_FRAMES;
            if self
                .active
                .as_ref()
                .is_some_and(|chunk| chunk.bucket != bucket)
            {
                let mut previous = self.active.take().unwrap();
                previous.silence_until(CHUNK_FRAMES)?;
                previous.finish(store)?;
            }
            if self.active.is_none() {
                self.active = Some(ActiveChunk::new(store, bucket, self.track)?);
            }
            let chunk = self.active.as_mut().unwrap();
            chunk.silence_until(frame % CHUNK_FRAMES)?;
            let count = samples.len().min((CHUNK_FRAMES - chunk.frames) as usize);
            for sample in &samples[..count] {
                chunk
                    .writer
                    .write_sample(to_pcm(*sample))
                    .map_err(wav_error)?;
            }
            chunk.frames += count as u64;
            frame += count as u64;
            self.next_frame = frame;
            samples = &samples[count..];
            if chunk.frames == CHUNK_FRAMES {
                self.active.take().unwrap().finish(store)?;
            }
        }
        Ok(())
    }
    pub fn checkpoint(&mut self) -> io::Result<()> {
        if let Some(chunk) = &mut self.active {
            chunk.checkpoint()?;
        }
        Ok(())
    }
    pub fn finish(&mut self, store: &mut MeetingStore) -> io::Result<()> {
        if let Some(chunk) = self.active.take() {
            chunk.finish(store)?;
        }
        Ok(())
    }
}

fn source_reader(
    store: &MeetingStore,
    index: Option<&usize>,
) -> io::Result<Option<hound::WavReader<BufReader<File>>>> {
    let Some(index) = index else {
        return Ok(None);
    };
    let reader = hound::WavReader::new(BufReader::new(store.open_registered_audio(*index)?))
        .map_err(wav_error)?;
    if reader.spec() != spec() {
        return Err(io::Error::other(
            "Source WAV format is not mono 48kHz PCM16",
        ));
    }
    Ok(Some(reader))
}

/// After both sources stop, create minute-sized mixed WAVs from their common
/// timeline. Keep raw source files for recovery. Missing audio remains silent;
/// averaging gives two voices headroom rather than clipping their sum.
pub fn mix_tracks(store: &mut MeetingStore, duration_ms: u64) -> io::Result<()> {
    if store
        .manifest()
        .audio
        .iter()
        .any(|audio| audio.track == AudioTrack::Mixed)
    {
        return Err(io::Error::other("Meeting already has mixed audio"));
    }
    let total_frames = duration_ms
        .checked_mul(AUDIO_SAMPLE_RATE as u64)
        .ok_or_else(|| io::Error::other("Meeting duration overflow"))?
        .div_ceil(1_000);
    let mut microphone = BTreeMap::new();
    let mut system = BTreeMap::new();
    for (index, chunk) in store.manifest().audio.iter().enumerate() {
        if chunk.offset_ms % AUDIO_CHUNK_MS != 0 || chunk.duration_ms > AUDIO_CHUNK_MS {
            return Err(io::Error::other(
                "Source audio is not on the meeting chunk grid",
            ));
        }
        let map = match chunk.track {
            AudioTrack::Microphone => &mut microphone,
            AudioTrack::System => &mut system,
            AudioTrack::Mixed => unreachable!(),
        };
        if map
            .insert(chunk.offset_ms / AUDIO_CHUNK_MS, index)
            .is_some()
        {
            return Err(io::Error::other("Duplicate source audio chunk"));
        }
    }
    let dual = store.manifest().mode == super::storage::MeetingMode::SystemAndMicrophone;
    for bucket in 0..total_frames.div_ceil(CHUNK_FRAMES) {
        let frames = CHUNK_FRAMES.min(total_frames - bucket * CHUNK_FRAMES);
        let mut mic = source_reader(store, microphone.get(&bucket))?;
        let mut sys = source_reader(store, system.get(&bucket))?;
        let mut mic_samples = mic.as_mut().map(|reader| reader.samples::<i16>());
        let mut sys_samples = sys.as_mut().map(|reader| reader.samples::<i16>());
        let mut mixed = ActiveChunk::new(store, bucket, AudioTrack::Mixed)?;
        for _ in 0..frames {
            let a = mic_samples
                .as_mut()
                .and_then(|samples| samples.next())
                .transpose()
                .map_err(wav_error)?
                .unwrap_or(0) as i32;
            let b = sys_samples
                .as_mut()
                .and_then(|samples| samples.next())
                .transpose()
                .map_err(wav_error)?
                .unwrap_or(0) as i32;
            let sample = if dual { (a + b) / 2 } else { a };
            mixed
                .writer
                .write_sample(sample as i16)
                .map_err(wav_error)?;
            mixed.frames += 1;
        }
        mixed.finish(store)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::storage::{MeetingManifest, MeetingMode, SavedMeeting};
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        root: std::path::PathBuf,
        store: MeetingStore,
    }
    impl Fixture {
        fn new(mode: MeetingMode) -> Self {
            let root = std::env::temp_dir().join(format!(
                "meeting-timeline-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&root).unwrap();
            let store = MeetingStore::create(
                &root.join("meetings"),
                MeetingManifest::new("test".into(), "test".into(), mode, "mic".into(), Some(1), 0),
            )
            .unwrap();
            Self { root, store }
        }
        fn samples(&self, index: usize) -> Vec<i16> {
            hound::WavReader::new(self.store.open_registered_audio(index).unwrap())
                .unwrap()
                .samples::<i16>()
                .map(Result::unwrap)
                .collect()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn packet(source: AudioSource, frame: u64, samples: &[f32]) -> AudioPacket {
        AudioPacket {
            source,
            host_time_ns: (frame as u128 * 1_000_000_000).div_ceil(AUDIO_SAMPLE_RATE as u128)
                as u64,
            samples: samples.to_vec(),
        }
    }

    #[test]
    fn aligns_timestamp_gaps_and_clips_duplicate_overlaps() {
        let mut fixture = Fixture::new(MeetingMode::Microphone);
        let mut source = SourceTimeline::new(AudioSource::Microphone, 0);
        source
            .push(
                packet(AudioSource::Microphone, 2, &[1.0, 0.5]),
                &mut fixture.store,
            )
            .unwrap();
        source
            .push(
                packet(AudioSource::Microphone, 3, &[-1.0, -0.5]),
                &mut fixture.store,
            )
            .unwrap();
        source.finish(&mut fixture.store).unwrap();
        assert_eq!(fixture.samples(0), [0, 0, 32767, 16384, -16384]);
    }

    #[test]
    fn trims_packets_before_recording_epoch() {
        let mut fixture = Fixture::new(MeetingMode::Microphone);
        let mut source = SourceTimeline::new(AudioSource::Microphone, 1_000_000_000);
        source
            .push(
                packet(AudioSource::Microphone, 48_000 - 2, &[1.0, 1.0, 0.25, 0.5]),
                &mut fixture.store,
            )
            .unwrap();
        source.finish(&mut fixture.store).unwrap();
        assert_eq!(fixture.samples(0), [8192, 16384]);
    }

    #[test]
    fn minute_boundary_splits_a_packet_and_keeps_shared_grid() {
        let mut fixture = Fixture::new(MeetingMode::SystemAndMicrophone);
        let mut source = SourceTimeline::new(AudioSource::System, 0);
        source
            .push(
                packet(AudioSource::System, CHUNK_FRAMES - 1, &[0.25, 0.5]),
                &mut fixture.store,
            )
            .unwrap();
        source.finish(&mut fixture.store).unwrap();
        assert_eq!(fixture.store.manifest().audio.len(), 2);
        assert_eq!(fixture.store.manifest().audio[1].offset_ms, 60_000);
        assert_eq!(fixture.samples(0).last(), Some(&8192));
        assert_eq!(fixture.samples(1), [16384]);
    }

    #[test]
    fn quiet_missing_minutes_do_not_allocate_source_files() {
        let mut fixture = Fixture::new(MeetingMode::SystemAndMicrophone);
        let mut source = SourceTimeline::new(AudioSource::System, 0);
        source
            .push(
                packet(AudioSource::System, 5 * CHUNK_FRAMES, &[0.5]),
                &mut fixture.store,
            )
            .unwrap();
        source.finish(&mut fixture.store).unwrap();
        assert_eq!(fixture.store.manifest().audio.len(), 1);
        assert_eq!(fixture.store.manifest().audio[0].offset_ms, 300_000);
        assert_eq!(fixture.samples(0), [16384]);
    }

    #[test]
    fn open_chunk_checkpoint_is_a_readable_wav_before_finalization() {
        let mut fixture = Fixture::new(MeetingMode::Microphone);
        let mut source = SourceTimeline::new(AudioSource::Microphone, 0);
        source
            .push(
                packet(AudioSource::Microphone, 0, &[0.25; 48]),
                &mut fixture.store,
            )
            .unwrap();
        source.checkpoint().unwrap();
        let path = fixture.root.join("meetings/test/audio/00000000.wav");
        assert_eq!(hound::WavReader::open(&path).unwrap().len(), 48);
        let recovery: serde_json::Value =
            serde_json::from_reader(File::open(path.with_extension("wav.json")).unwrap()).unwrap();
        assert_eq!(recovery["track"], "microphone");
        assert_eq!(recovery["offsetMs"], 0);
        assert_eq!(recovery["path"], "audio/00000000.wav");
        assert!(fixture.store.manifest().audio.is_empty()); // preserved, not yet registered
        source.finish(&mut fixture.store).unwrap();
    }

    #[test]
    fn mixed_audio_aligns_sources_and_retains_raw_but_only_mixed_is_transcribable() {
        let mut fixture = Fixture::new(MeetingMode::SystemAndMicrophone);
        let mut mic = SourceTimeline::new(AudioSource::Microphone, 0);
        let mut sys = SourceTimeline::new(AudioSource::System, 0);
        mic.push(
            packet(AudioSource::Microphone, 0, &[1.0, 1.0]),
            &mut fixture.store,
        )
        .unwrap();
        sys.push(packet(AudioSource::System, 1, &[1.0]), &mut fixture.store)
            .unwrap();
        mic.finish(&mut fixture.store).unwrap();
        sys.finish(&mut fixture.store).unwrap();
        mix_tracks(&mut fixture.store, 1).unwrap();
        let samples = fixture.samples(2);
        assert_eq!(&samples[..3], &[16383, 32767, 0]);
        assert_eq!(samples.len(), 48);
        fixture.store.finish(1, false).unwrap();
        let saved = SavedMeeting::load(&fixture.root.join("meetings"), "test").unwrap();
        assert!(saved.open_audio_for_transcription(0).is_err());
        assert!(saved.open_audio_for_transcription(2).is_ok());
        assert!(mix_tracks(&mut fixture.store, 1).is_err());
    }

    #[test]
    fn microphone_only_mix_preserves_volume_and_fills_trailing_silence() {
        let mut fixture = Fixture::new(MeetingMode::Microphone);
        let mut mic = SourceTimeline::new(AudioSource::Microphone, 0);
        mic.push(
            packet(AudioSource::Microphone, 0, &[1.0, -1.0]),
            &mut fixture.store,
        )
        .unwrap();
        mic.finish(&mut fixture.store).unwrap();
        mix_tracks(&mut fixture.store, 1).unwrap();
        assert_eq!(&fixture.samples(1)[..3], &[32767, -32767, 0]);
    }

    #[test]
    fn resumed_microphone_gap_is_not_silently_filled_as_a_successful_recording() {
        let mut fixture = Fixture::new(MeetingMode::Microphone);
        let mut source = SourceTimeline::new(AudioSource::Microphone, 0);
        source
            .push(
                packet(AudioSource::Microphone, 0, &[0.25; 48]),
                &mut fixture.store,
            )
            .unwrap();
        let error = source
            .push(
                packet(
                    AudioSource::Microphone,
                    11 * AUDIO_SAMPLE_RATE as u64,
                    &[0.5],
                ),
                &mut fixture.store,
            )
            .unwrap_err();
        assert!(error.to_string().contains("gap longer than ten seconds"));
        source.finish(&mut fixture.store).unwrap();
        assert_eq!(fixture.samples(0), vec![8192; 48]);
    }

    #[test]
    fn wrong_source_packets_are_rejected_without_audio_registration() {
        let mut fixture = Fixture::new(MeetingMode::Microphone);
        let mut mic = SourceTimeline::new(AudioSource::Microphone, 0);
        assert!(mic
            .push(packet(AudioSource::System, 0, &[1.0]), &mut fixture.store)
            .is_err());
        assert!(fixture.store.manifest().audio.is_empty());
    }
}
