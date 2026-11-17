use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use bevy::audio::{
    AddAudioSource, AudioSource, ChannelCount, Decodable, Sample, SampleRate, SeekError, Source,
};
use bevy::prelude::{App, Asset, AssetId, Assets, Handle, Plugin, Resource, TypePath};

use super::sound::TrackRole;

/// Decoded interleaved floating-point PCM buffer with channel count and sample rate.
#[derive(Clone, Debug)]
pub struct PcmBuffer {
    pub samples: Arc<[f32]>,
    pub channels: ChannelCount,
    pub sample_rate: SampleRate,
}

impl PcmBuffer {
    /// Decodes an [`AudioSource`] completely into interleaved `f32` PCM samples.
    pub fn decode_from_audio_source(src: &AudioSource) -> Option<Self> {
        if src.bytes.is_empty() {
            return None;
        }
        let decoder = src.decoder();
        let channels = decoder.channels();
        let sample_rate = decoder.sample_rate();
        let mut samples: Vec<f32> = decoder.collect();
        let ch = usize::from(channels.get());
        let aligned_len = samples.len() - (samples.len() % ch);
        samples.truncate(aligned_len);
        if samples.is_empty() {
            return None;
        }
        Some(Self {
            samples: Arc::from(samples),
            channels,
            sample_rate,
        })
    }
}

/// Converts `buf` into interleaved PCM matching `target_channels` and `target_rate`.
///
/// Supports arbitrary channel counts (`1 -> N`, `N -> 1`, `N -> M`) and performs
/// linear resampling when `buf.sample_rate != target_rate`.
pub fn normalize_pcm(
    buf: &PcmBuffer,
    target_channels: ChannelCount,
    target_rate: SampleRate,
) -> Arc<[f32]> {
    let src_ch = usize::from(buf.channels.get());
    let dst_ch = usize::from(target_channels.get());
    let src_frames = buf.samples.len() / src_ch;
    if src_frames == 0 {
        return Arc::from([]);
    }
    if buf.channels == target_channels
        && buf.sample_rate == target_rate
        && buf.samples.len() == src_frames * src_ch
    {
        return Arc::clone(&buf.samples);
    }

    let src_rate = buf.sample_rate.get();
    let dst_rate = target_rate.get();
    let dst_frames = if src_rate == dst_rate {
        src_frames
    } else {
        ((src_frames as f64 * f64::from(dst_rate)) / f64::from(src_rate))
            .round()
            .max(1.0) as usize
    };

    let mut out = Vec::with_capacity(dst_frames.saturating_mul(dst_ch));
    for dst_idx in 0..dst_frames {
        if src_rate == dst_rate {
            let frame_start = dst_idx * src_ch;
            let frame = &buf.samples[frame_start..frame_start + src_ch];
            for c in 0..dst_ch {
                out.push(converted_channel_sample(frame, src_ch, dst_ch, c));
            }
        } else {
            // Multiply before dividing in f64 to avoid cumulative phase drift on long tracks.
            let src_pos = (dst_idx as f64 * f64::from(src_rate)) / f64::from(dst_rate);
            let i0 = (src_pos.floor() as usize).min(src_frames - 1);
            let i1 = (i0 + 1).min(src_frames - 1);
            let frac = (src_pos - i0 as f64).clamp(0.0, 1.0) as f32;
            let f0 = &buf.samples[i0 * src_ch..(i0 + 1) * src_ch];
            let f1 = &buf.samples[i1 * src_ch..(i1 + 1) * src_ch];
            for c in 0..dst_ch {
                let s0 = converted_channel_sample(f0, src_ch, dst_ch, c);
                let s1 = converted_channel_sample(f1, src_ch, dst_ch, c);
                out.push(s0 + (s1 - s0) * frac);
            }
        }
    }
    Arc::from(out)
}

#[inline]
fn converted_channel_sample(frame: &[f32], src_ch: usize, dst_ch: usize, out_ch: usize) -> f32 {
    if src_ch == dst_ch {
        return frame[out_ch];
    }
    if src_ch == 1 {
        return frame[0];
    }
    if dst_ch == 1 {
        return frame.iter().copied().sum::<f32>() / src_ch as f32;
    }
    if src_ch < dst_ch {
        return if out_ch < src_ch { frame[out_ch] } else { 0.0 };
    }
    let mut sum = 0.0_f32;
    let mut count = 0usize;
    for idx in (out_ch..src_ch).step_by(dst_ch) {
        sum += frame[idx];
        count += 1;
    }
    if count > 0 { sum / count as f32 } else { 0.0 }
}

/// Lock-free per-track volume control and normalized PCM buffer inside [`SyncedSongAudio`].
#[derive(Debug)]
pub struct MixTrackControl {
    pub role: TrackRole,
    pub base_volume: f32,
    volume_bits: AtomicU32,
    pub offset_frames: isize,
    pub samples: Arc<[f32]>,
}

impl MixTrackControl {
    pub fn new(
        role: TrackRole,
        base_volume: f32,
        initial_volume: f32,
        offset_frames: isize,
        samples: Arc<[f32]>,
    ) -> Self {
        let ctrl = Self {
            role,
            base_volume,
            volume_bits: AtomicU32::new(0),
            offset_frames,
            samples,
        };
        ctrl.set_volume(initial_volume);
        ctrl
    }

    #[inline]
    pub fn set_volume(&self, vol: f32) {
        let safe = if vol.is_finite() { vol.max(0.0) } else { 0.0 };
        self.volume_bits.store(safe.to_bits(), Ordering::Relaxed);
    }

    #[inline]
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }
}

/// Sample-locked multi-track audio asset mixed on demand from normalized PCM stems.
#[derive(Asset, TypePath, Clone, Debug)]
pub struct SyncedSongAudio {
    tracks: Arc<[MixTrackControl]>,
    channels: ChannelCount,
    sample_rate: SampleRate,
    total_frames: usize,
    cursor_frames: Arc<AtomicUsize>,
}

impl SyncedSongAudio {
    pub fn new(
        tracks: Arc<[MixTrackControl]>,
        channels: ChannelCount,
        sample_rate: SampleRate,
        cursor_frames: Arc<AtomicUsize>,
    ) -> Self {
        let ch = usize::from(channels.get());
        let total_frames = tracks
            .iter()
            .filter_map(|track| {
                let track_frames = (track.samples.len() / ch) as isize;
                let end = track.offset_frames.saturating_add(track_frames);
                (end > 0).then_some(end as usize)
            })
            .max()
            .unwrap_or(0);
        Self {
            tracks,
            channels,
            sample_rate,
            total_frames,
            cursor_frames,
        }
    }
}

impl Decodable for SyncedSongAudio {
    type Decoder = SyncedMixSource;

    fn decoder(&self) -> Self::Decoder {
        self.cursor_frames.store(0, Ordering::Relaxed);
        SyncedMixSource {
            tracks: Arc::clone(&self.tracks),
            channels: self.channels,
            sample_rate: self.sample_rate,
            total_frames: self.total_frames,
            cursor_frames: Arc::clone(&self.cursor_frames),
            local_frame: 0,
            channel_idx: 0,
        }
    }
}

/// Streaming decoder produced by [`SyncedSongAudio`], mixing all tracks at a shared sample clock.
#[derive(Clone, Debug)]
pub struct SyncedMixSource {
    tracks: Arc<[MixTrackControl]>,
    channels: ChannelCount,
    sample_rate: SampleRate,
    total_frames: usize,
    cursor_frames: Arc<AtomicUsize>,
    local_frame: usize,
    channel_idx: usize,
}

impl Iterator for SyncedMixSource {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.local_frame >= self.total_frames {
            return None;
        }
        let ch = usize::from(self.channels.get());
        let frame_signed = self.local_frame as isize;
        let mut mixed = 0.0_f32;
        for track in self.tracks.iter() {
            let track_frames = (track.samples.len() / ch) as isize;
            let rel = frame_signed - track.offset_frames;
            if rel >= 0 && rel < track_frames {
                let sample_idx = rel as usize * ch + self.channel_idx;
                if let Some(&sample) = track.samples.get(sample_idx) {
                    mixed += sample * track.volume();
                }
            }
        }
        let out = mixed.clamp(-1.0, 1.0);

        self.channel_idx += 1;
        if self.channel_idx >= ch {
            self.channel_idx = 0;
            self.local_frame += 1;
            self.cursor_frames.store(self.local_frame, Ordering::Relaxed);
        }

        Some(out)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let ch = usize::from(self.channels.get());
        let remaining = self
            .total_frames
            .saturating_sub(self.local_frame)
            .saturating_mul(ch)
            .saturating_sub(self.channel_idx);
        (remaining, Some(remaining))
    }
}

impl Source for SyncedMixSource {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            self.total_frames as f64 / f64::from(self.sample_rate.get()),
        ))
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        let target_frame =
            ((pos.as_secs_f64() * f64::from(self.sample_rate.get())).round() as usize)
                .min(self.total_frames);
        self.local_frame = target_frame;
        self.channel_idx = 0;
        self.cursor_frames.store(target_frame, Ordering::Relaxed);
        Ok(())
    }
}

/// Pre-decoded PCM sound effect asset supporting O(1) sample-accurate seeking.
#[derive(Asset, TypePath, Clone, Debug)]
pub struct PcmSfxAudio {
    pub buffer: PcmBuffer,
}

impl PcmSfxAudio {
    pub fn new(buffer: PcmBuffer) -> Self {
        Self { buffer }
    }

    pub fn from_audio_source(src: &AudioSource) -> Option<Self> {
        PcmBuffer::decode_from_audio_source(src).map(Self::new)
    }
}

impl Decodable for PcmSfxAudio {
    type Decoder = PcmSfxSource;

    fn decoder(&self) -> Self::Decoder {
        let ch = usize::from(self.buffer.channels.get());
        let total_frames = self.buffer.samples.len() / ch;
        PcmSfxSource {
            samples: Arc::clone(&self.buffer.samples),
            channels: self.buffer.channels,
            sample_rate: self.buffer.sample_rate,
            total_frames,
            local_frame: 0,
            channel_idx: 0,
        }
    }
}

/// Streaming decoder over a [`PcmSfxAudio`] buffer.
#[derive(Clone, Debug)]
pub struct PcmSfxSource {
    samples: Arc<[f32]>,
    channels: ChannelCount,
    sample_rate: SampleRate,
    total_frames: usize,
    local_frame: usize,
    channel_idx: usize,
}

impl Iterator for PcmSfxSource {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.local_frame >= self.total_frames {
            return None;
        }
        let ch = usize::from(self.channels.get());
        let idx = self.local_frame * ch + self.channel_idx;
        let sample = self.samples.get(idx).copied()?;
        self.channel_idx += 1;
        if self.channel_idx >= ch {
            self.channel_idx = 0;
            self.local_frame += 1;
        }
        Some(sample)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let ch = usize::from(self.channels.get());
        let remaining = self
            .total_frames
            .saturating_sub(self.local_frame)
            .saturating_mul(ch)
            .saturating_sub(self.channel_idx);
        (remaining, Some(remaining))
    }
}

impl Source for PcmSfxSource {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            self.total_frames as f64 / f64::from(self.sample_rate.get()),
        ))
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        let target_frame =
            ((pos.as_secs_f64() * f64::from(self.sample_rate.get())).round() as usize)
                .min(self.total_frames);
        self.local_frame = target_frame;
        self.channel_idx = 0;
        Ok(())
    }
}

/// Cache mapping raw [`AudioSource`] handles to pre-decoded [`PcmSfxAudio`] assets.
#[derive(Resource, Default, Debug)]
pub struct PcmSfxCache {
    cache: HashMap<AssetId<AudioSource>, Handle<PcmSfxAudio>>,
}

impl PcmSfxCache {
    pub fn get_or_insert(
        &mut self,
        handle: &Handle<AudioSource>,
        audio_sources: &Assets<AudioSource>,
        pcm_assets: &mut Assets<PcmSfxAudio>,
    ) -> Option<Handle<PcmSfxAudio>> {
        let id = handle.id();
        if let Some(existing) = self.cache.get(&id)
            && pcm_assets.contains(existing.id())
        {
            return Some(existing.clone());
        }
        let source = audio_sources.get(id)?;
        let pcm = PcmSfxAudio::from_audio_source(source)?;
        let pcm_handle = pcm_assets.add(pcm);
        self.cache.insert(id, pcm_handle.clone());
        Some(pcm_handle)
    }
}

/// Registers custom PCM audio sources and the [`PcmSfxCache`] resource with Bevy.
pub struct OxideAudioPlugin;

impl Plugin for OxideAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<SyncedSongAudio>()
            .add_audio_source::<PcmSfxAudio>()
            .init_resource::<PcmSfxCache>();
    }
}
