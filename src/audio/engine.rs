use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};
use thiserror::Error;

use crate::audio::sample_bank::{SampleBank, SampleBankError};
use crate::audio::scheduler::{BeatEvent, BeatScheduler};
use crate::config::{
    AppConfig, BPM_MAX, BPM_MIN, CLICK_OFFSET_MAX_MS, CLICK_OFFSET_MIN_MS, OUTPUT_BOOST_DB_MAX,
};

const MAX_VOICES: usize = 16;

#[derive(Debug, Clone, Copy)]
pub struct AudioEvent {
    pub beat: BeatEvent,
}

#[derive(Debug, Clone)]
pub struct AudioDiagnostics {
    pub host: String,
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
    pub callbacks: u64,
    pub stream_errors: u64,
    pub event_drops: u64,
    pub voice_drops: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Error)]
pub enum AudioInitError {
    #[error("既定の出力デバイスが見つかりません")]
    NoOutputDevice,
    #[error("音声初期化に失敗しました: {0}")]
    Cpal(#[from] cpal::Error),
    #[error("内蔵クリック音源を読み込めませんでした: {0}")]
    SampleBank(#[from] SampleBankError),
    #[error("未対応の出力サンプル形式です: {0}")]
    UnsupportedSampleFormat(SampleFormat),
}

pub struct AudioEngine {
    _stream: Stream,
    shared: Arc<AudioShared>,
    diagnostics: Arc<DiagnosticsShared>,
    event_consumer: Consumer<AudioEvent>,
    info: StreamInfo,
}

impl AudioEngine {
    pub fn new(config: &AppConfig) -> Result<Self, AudioInitError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioInitError::NoOutputDevice)?;
        let supported_config = device.default_output_config()?;
        let sample_format = supported_config.sample_format();
        let stream_config: StreamConfig = supported_config.into();
        let sample_rate = stream_config.sample_rate;
        let channels = stream_config.channels;
        let sample_bank = SampleBank::from_config(config, sample_rate)?;
        let (event_producer, event_consumer) = RingBuffer::<AudioEvent>::new(256);

        let shared = Arc::new(AudioShared::from_config(config));
        let diagnostics = Arc::new(DiagnosticsShared::default());
        let info = StreamInfo {
            host: host.id().to_string(),
            device: device.to_string(),
            sample_rate,
            channels,
            sample_format: sample_format.to_string(),
        };

        let render_state = RenderState::new(
            sample_rate,
            channels as usize,
            shared.clone(),
            diagnostics.clone(),
            event_producer,
            sample_bank,
        );

        let stream = match sample_format {
            SampleFormat::F32 => {
                build_stream::<f32>(&device, stream_config, render_state, diagnostics.clone())?
            }
            SampleFormat::F64 => {
                build_stream::<f64>(&device, stream_config, render_state, diagnostics.clone())?
            }
            SampleFormat::I8 => {
                build_stream::<i8>(&device, stream_config, render_state, diagnostics.clone())?
            }
            SampleFormat::I16 => {
                build_stream::<i16>(&device, stream_config, render_state, diagnostics.clone())?
            }
            SampleFormat::I24 => build_stream::<cpal::I24>(
                &device,
                stream_config,
                render_state,
                diagnostics.clone(),
            )?,
            SampleFormat::I32 => {
                build_stream::<i32>(&device, stream_config, render_state, diagnostics.clone())?
            }
            SampleFormat::I64 => {
                build_stream::<i64>(&device, stream_config, render_state, diagnostics.clone())?
            }
            SampleFormat::U8 => {
                build_stream::<u8>(&device, stream_config, render_state, diagnostics.clone())?
            }
            SampleFormat::U16 => {
                build_stream::<u16>(&device, stream_config, render_state, diagnostics.clone())?
            }
            SampleFormat::U24 => build_stream::<cpal::U24>(
                &device,
                stream_config,
                render_state,
                diagnostics.clone(),
            )?,
            SampleFormat::U32 => {
                build_stream::<u32>(&device, stream_config, render_state, diagnostics.clone())?
            }
            SampleFormat::U64 => {
                build_stream::<u64>(&device, stream_config, render_state, diagnostics.clone())?
            }
            unsupported => return Err(AudioInitError::UnsupportedSampleFormat(unsupported)),
        };

        stream.play()?;

        Ok(Self {
            _stream: stream,
            shared,
            diagnostics,
            event_consumer,
            info,
        })
    }

    pub fn set_running(&self, running: bool) {
        self.shared.running.store(running, Ordering::Relaxed);
    }

    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Relaxed)
    }

    pub fn set_bpm_milli(&self, bpm_milli: u32) {
        self.shared
            .bpm_milli
            .store(bpm_milli.clamp(BPM_MIN, BPM_MAX), Ordering::Relaxed);
    }

    pub fn set_output_level(&self, volume_percent: u8, output_boost_db: f32) {
        let gain = output_gain(volume_percent, output_boost_db);
        self.shared
            .gain_bits
            .store(gain.to_bits(), Ordering::Relaxed);
    }

    pub fn set_time_signature(&self, beats_per_bar: u8, beat_unit: u8) {
        self.shared
            .beats_per_bar
            .store(u32::from(beats_per_bar.clamp(1, 16)), Ordering::Relaxed);
        self.shared.beat_unit.store(
            u32::from(match beat_unit {
                2 | 4 | 8 | 16 => beat_unit,
                _ => 4,
            }),
            Ordering::Relaxed,
        );
    }

    pub fn set_click_timing_offset_ms(&self, offset_ms: i32) {
        self.shared.click_offset_ms.store(
            offset_ms.clamp(CLICK_OFFSET_MIN_MS, CLICK_OFFSET_MAX_MS),
            Ordering::Relaxed,
        );
    }

    pub fn set_subdivision(&self, subdivision: u8) {
        self.shared
            .subdivision
            .store(u32::from(subdivision.clamp(1, 8)), Ordering::Relaxed);
    }

    pub fn poll_events(&mut self) -> Vec<AudioEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_consumer.pop() {
            events.push(event);
        }
        events
    }

    pub fn diagnostics(&self) -> AudioDiagnostics {
        AudioDiagnostics {
            host: self.info.host.clone(),
            device: self.info.device.clone(),
            sample_rate: self.info.sample_rate,
            channels: self.info.channels,
            sample_format: self.info.sample_format.clone(),
            callbacks: self.diagnostics.callbacks.load(Ordering::Relaxed),
            stream_errors: self.diagnostics.stream_errors.load(Ordering::Relaxed),
            event_drops: self.diagnostics.event_drops.load(Ordering::Relaxed),
            voice_drops: self.diagnostics.voice_drops.load(Ordering::Relaxed),
            last_error: self
                .diagnostics
                .last_error
                .lock()
                .ok()
                .and_then(|err| err.clone()),
        }
    }
}

#[derive(Debug, Clone)]
struct StreamInfo {
    host: String,
    device: String,
    sample_rate: u32,
    channels: u16,
    sample_format: String,
}

#[derive(Debug)]
struct AudioShared {
    running: AtomicBool,
    bpm_milli: AtomicU32,
    gain_bits: AtomicU32,
    beats_per_bar: AtomicU32,
    beat_unit: AtomicU32,
    click_offset_ms: AtomicI32,
    subdivision: AtomicU32,
}

impl AudioShared {
    fn from_config(config: &AppConfig) -> Self {
        Self {
            running: AtomicBool::new(false),
            bpm_milli: AtomicU32::new(config.bpm_milli.clamp(BPM_MIN, BPM_MAX)),
            gain_bits: AtomicU32::new(
                output_gain(config.volume_percent, config.audio.output_boost_db).to_bits(),
            ),
            beats_per_bar: AtomicU32::new(u32::from(
                config.time_signature.beats_per_bar.clamp(1, 16),
            )),
            beat_unit: AtomicU32::new(u32::from(config.time_signature.beat_unit)),
            click_offset_ms: AtomicI32::new(
                config
                    .audio
                    .click_timing_offset_ms
                    .clamp(CLICK_OFFSET_MIN_MS, CLICK_OFFSET_MAX_MS),
            ),
            subdivision: AtomicU32::new(u32::from(config.audio.subdivision.clamp(1, 8))),
        }
    }
}

#[derive(Debug, Default)]
struct DiagnosticsShared {
    callbacks: AtomicU64,
    stream_errors: AtomicU64,
    event_drops: AtomicU64,
    voice_drops: AtomicU64,
    last_error: Mutex<Option<String>>,
}

struct RenderState {
    scheduler: BeatScheduler,
    shared: Arc<AudioShared>,
    diagnostics: Arc<DiagnosticsShared>,
    event_producer: Producer<AudioEvent>,
    sample_bank: SampleBank,
    channels: usize,
    voices: Vec<Voice>,
    was_running: bool,
    current_gain: f32,
    gain_step: f32,
}

impl RenderState {
    fn new(
        sample_rate: u32,
        channels: usize,
        shared: Arc<AudioShared>,
        diagnostics: Arc<DiagnosticsShared>,
        event_producer: Producer<AudioEvent>,
        sample_bank: SampleBank,
    ) -> Self {
        let current_gain = f32::from_bits(shared.gain_bits.load(Ordering::Relaxed));
        Self {
            scheduler: BeatScheduler::new(sample_rate),
            shared,
            diagnostics,
            event_producer,
            sample_bank,
            channels,
            voices: vec![Voice::default(); MAX_VOICES],
            was_running: false,
            current_gain,
            gain_step: 1.0 / (sample_rate as f32 * 0.005).max(1.0),
        }
    }

    fn write_output<T>(&mut self, data: &mut [T])
    where
        T: Sample + FromSample<f32>,
    {
        self.diagnostics.callbacks.fetch_add(1, Ordering::Relaxed);

        for frame in data.chunks_mut(self.channels) {
            let value = self.render_frame();
            let sample = T::from_sample(value);
            for output in frame {
                *output = sample;
            }
        }
    }

    fn render_frame(&mut self) -> f32 {
        let running = self.shared.running.load(Ordering::Relaxed);
        if running && !self.was_running {
            self.scheduler.start();
        } else if !running && self.was_running {
            self.scheduler.stop();
            for voice in &mut self.voices {
                voice.active = false;
            }
        }
        self.was_running = running;

        if running {
            let bpm_milli = self.shared.bpm_milli.load(Ordering::Relaxed);
            let beats_per_bar = self.shared.beats_per_bar.load(Ordering::Relaxed) as u8;
            let beat_unit = self.shared.beat_unit.load(Ordering::Relaxed) as u8;
            let click_offset_ms = self.shared.click_offset_ms.load(Ordering::Relaxed);
            let subdivision = self.shared.subdivision.load(Ordering::Relaxed) as u8;
            if let Some(beat) = self.scheduler.advance_frame(
                bpm_milli,
                beats_per_bar,
                beat_unit,
                subdivision,
                click_offset_ms,
            ) {
                self.trigger_voice(beat);
            }
        }

        self.advance_gain();
        (self.mix_voices() * self.current_gain).clamp(-1.0, 1.0)
    }

    fn trigger_voice(&mut self, beat: BeatEvent) {
        if let Some(voice) = self.voices.iter_mut().find(|voice| !voice.active) {
            voice.active = true;
            voice.position = 0;
            voice.sample = if beat.is_accent {
                VoiceSample::Accent
            } else if beat.subdivision_index > 0 {
                VoiceSample::Subdivision
            } else {
                VoiceSample::Normal
            };
        } else {
            self.diagnostics.voice_drops.fetch_add(1, Ordering::Relaxed);
        }

        if self.event_producer.push(AudioEvent { beat }).is_err() {
            self.diagnostics.event_drops.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn mix_voices(&mut self) -> f32 {
        let mut mixed = 0.0;
        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            let (samples, sample_gain) = match voice.sample {
                VoiceSample::Normal => (&self.sample_bank.normal, 1.0),
                VoiceSample::Accent => (&self.sample_bank.accent, 1.0),
                VoiceSample::Subdivision => (&self.sample_bank.normal, 0.6),
            };

            if let Some(sample) = samples.get(voice.position) {
                mixed += *sample * sample_gain;
                voice.position += 1;
            } else {
                voice.active = false;
            }
        }
        mixed
    }

    fn advance_gain(&mut self) {
        let target = f32::from_bits(self.shared.gain_bits.load(Ordering::Relaxed))
            .clamp(0.0, db_to_gain(OUTPUT_BOOST_DB_MAX));
        if (self.current_gain - target).abs() <= self.gain_step {
            self.current_gain = target;
        } else if self.current_gain < target {
            self.current_gain += self.gain_step;
        } else {
            self.current_gain -= self.gain_step;
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Voice {
    active: bool,
    position: usize,
    sample: VoiceSample,
}

#[derive(Clone, Copy, Debug, Default)]
enum VoiceSample {
    #[default]
    Normal,
    Accent,
    Subdivision,
}

fn output_gain(volume_percent: u8, output_boost_db: f32) -> f32 {
    let volume = f32::from(volume_percent.min(100)) / 100.0;
    volume * db_to_gain(output_boost_db.clamp(0.0, OUTPUT_BOOST_DB_MAX))
}

fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

fn build_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut render_state: RenderState,
    diagnostics: Arc<DiagnosticsShared>,
) -> Result<Stream, cpal::Error>
where
    T: Sample + FromSample<f32> + SizedSample + Send + 'static,
{
    device.build_output_stream(
        config,
        move |data: &mut [T], _| render_state.write_output(data),
        move |err| {
            diagnostics.stream_errors.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut last_error) = diagnostics.last_error.lock() {
                *last_error = Some(err.to_string());
            }
        },
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::output_gain;

    #[test]
    fn twelve_db_boost_uses_expected_linear_gain() {
        let gain = output_gain(100, 12.0);
        assert!((gain - 3.981_071_7).abs() < 0.0001);
        assert!((output_gain(50, 12.0) - gain * 0.5).abs() < 0.0001);
    }
}
