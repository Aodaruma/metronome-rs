use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};
use thiserror::Error;

use crate::audio::sample_bank::{SampleBank, SampleBankError, TransientAnalysis};
use crate::audio::scheduler::{
    BeatEvent, BeatScheduler, SwingGrid as SchedulerSwingGrid, SwingSettings,
};
use crate::config::{
    AppConfig, BEAT_UNIT_MAX, BEAT_UNIT_MIN, BPM_MAX, BPM_MIN, CLICK_OFFSET_MAX_MS,
    CLICK_OFFSET_MIN_MS, OUTPUT_VOLUME_DB_MAX, OUTPUT_VOLUME_DB_MIN, SWING_AMOUNT_MAX,
    SWING_AMOUNT_MIN, SoundTimingSettings, SwingGrid as ConfigSwingGrid,
};

const MAX_VOICES: usize = 64;
const MAX_SCHEDULED_EVENTS: usize = 128;
const SOURCE_OFFSET_LOOKAHEAD_MS: u64 = CLICK_OFFSET_MAX_MS as u64;

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
    transient_analyses: [TransientAnalysis; 3],
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
        let transient_analyses = [
            sample_bank.normal_transient,
            sample_bank.accent_transient,
            sample_bank.subdivision_transient,
        ];
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
            transient_analyses,
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

    pub fn set_output_levels(
        &self,
        normal_volume_percent: u8,
        accent_volume_percent: u8,
        subdivision_volume_percent: u8,
        output_volume_db: f32,
    ) {
        self.shared.normal_gain_bits.store(
            percent_gain(normal_volume_percent).to_bits(),
            Ordering::Relaxed,
        );
        self.shared.accent_gain_bits.store(
            percent_gain(accent_volume_percent).to_bits(),
            Ordering::Relaxed,
        );
        self.shared.subdivision_gain_bits.store(
            percent_gain(subdivision_volume_percent).to_bits(),
            Ordering::Relaxed,
        );
        self.shared.boost_gain_bits.store(
            output_volume_gain(output_volume_db).to_bits(),
            Ordering::Relaxed,
        );
    }

    pub fn set_time_signature(&self, beats_per_bar: u8, beat_unit: u8) {
        self.shared
            .beats_per_bar
            .store(u32::from(beats_per_bar.clamp(1, 16)), Ordering::Relaxed);
        self.shared.beat_unit.store(
            u32::from(beat_unit.clamp(BEAT_UNIT_MIN, BEAT_UNIT_MAX)),
            Ordering::Relaxed,
        );
    }

    pub fn set_swing(&self, grid: ConfigSwingGrid, amount_percent: i16) {
        self.shared
            .swing_grid
            .store(swing_grid_code(grid), Ordering::Relaxed);
        self.shared.swing_amount_percent.store(
            i32::from(amount_percent.clamp(SWING_AMOUNT_MIN, SWING_AMOUNT_MAX)),
            Ordering::Relaxed,
        );
    }

    pub fn set_source_timing(
        &self,
        normal: SoundTimingSettings,
        accent: SoundTimingSettings,
        subdivision: SoundTimingSettings,
    ) {
        self.shared.set_source_timing([normal, accent, subdivision]);
    }

    pub fn transient_analyses(&self) -> [TransientAnalysis; 3] {
        self.transient_analyses
    }

    pub fn set_subdivision(&self, subdivision: u8) {
        self.shared
            .subdivision
            .store(u32::from(subdivision.clamp(1, 8)), Ordering::Relaxed);
    }

    pub fn set_accent_enabled(&self, enabled: bool) {
        self.shared.accent_enabled.store(enabled, Ordering::Relaxed);
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
    boost_gain_bits: AtomicU32,
    normal_gain_bits: AtomicU32,
    accent_gain_bits: AtomicU32,
    subdivision_gain_bits: AtomicU32,
    beats_per_bar: AtomicU32,
    beat_unit: AtomicU32,
    subdivision: AtomicU32,
    accent_enabled: AtomicBool,
    swing_grid: AtomicU32,
    swing_amount_percent: AtomicI32,
    normal_offset_ms: AtomicI32,
    accent_offset_ms: AtomicI32,
    subdivision_offset_ms: AtomicI32,
    normal_auto_align: AtomicBool,
    accent_auto_align: AtomicBool,
    subdivision_auto_align: AtomicBool,
}

impl AudioShared {
    fn from_config(config: &AppConfig) -> Self {
        let timing = [
            config.sound.timing_for(&config.sound.normal_source_id()),
            config.sound.timing_for(&config.sound.accent_source_id()),
            config
                .sound
                .timing_for(&config.sound.subdivision_source_id()),
        ];
        Self {
            running: AtomicBool::new(false),
            bpm_milli: AtomicU32::new(config.bpm_milli.clamp(BPM_MIN, BPM_MAX)),
            boost_gain_bits: AtomicU32::new(
                output_volume_gain(config.audio.output_volume_db).to_bits(),
            ),
            normal_gain_bits: AtomicU32::new(
                percent_gain(config.sound.normal_volume_percent).to_bits(),
            ),
            accent_gain_bits: AtomicU32::new(
                percent_gain(config.sound.accent_volume_percent).to_bits(),
            ),
            subdivision_gain_bits: AtomicU32::new(
                percent_gain(config.sound.subdivision_volume_percent).to_bits(),
            ),
            beats_per_bar: AtomicU32::new(u32::from(
                config.time_signature.beats_per_bar.clamp(1, 16),
            )),
            beat_unit: AtomicU32::new(u32::from(config.time_signature.beat_unit)),
            subdivision: AtomicU32::new(u32::from(config.audio.subdivision.clamp(1, 8))),
            accent_enabled: AtomicBool::new(config.sound.accent_enabled),
            swing_grid: AtomicU32::new(swing_grid_code(config.audio.swing.grid)),
            swing_amount_percent: AtomicI32::new(i32::from(
                config
                    .audio
                    .swing
                    .amount_percent
                    .clamp(SWING_AMOUNT_MIN, SWING_AMOUNT_MAX),
            )),
            normal_offset_ms: AtomicI32::new(timing[0].manual_offset_ms),
            accent_offset_ms: AtomicI32::new(timing[1].manual_offset_ms),
            subdivision_offset_ms: AtomicI32::new(timing[2].manual_offset_ms),
            normal_auto_align: AtomicBool::new(timing[0].auto_align_transient),
            accent_auto_align: AtomicBool::new(timing[1].auto_align_transient),
            subdivision_auto_align: AtomicBool::new(timing[2].auto_align_transient),
        }
    }

    fn set_source_timing(&self, settings: [SoundTimingSettings; 3]) {
        for (offset, auto, setting) in [
            (&self.normal_offset_ms, &self.normal_auto_align, settings[0]),
            (&self.accent_offset_ms, &self.accent_auto_align, settings[1]),
            (
                &self.subdivision_offset_ms,
                &self.subdivision_auto_align,
                settings[2],
            ),
        ] {
            offset.store(
                setting
                    .manual_offset_ms
                    .clamp(CLICK_OFFSET_MIN_MS, CLICK_OFFSET_MAX_MS),
                Ordering::Relaxed,
            );
            auto.store(setting.auto_align_transient, Ordering::Relaxed);
        }
    }

    fn timing_for_sample(&self, sample: VoiceSample) -> (i32, bool) {
        match sample {
            VoiceSample::Normal => (
                self.normal_offset_ms.load(Ordering::Relaxed),
                self.normal_auto_align.load(Ordering::Relaxed),
            ),
            VoiceSample::Accent => (
                self.accent_offset_ms.load(Ordering::Relaxed),
                self.accent_auto_align.load(Ordering::Relaxed),
            ),
            VoiceSample::Subdivision => (
                self.subdivision_offset_ms.load(Ordering::Relaxed),
                self.subdivision_auto_align.load(Ordering::Relaxed),
            ),
        }
    }

    fn swing(&self) -> SwingSettings {
        SwingSettings::new(
            scheduler_swing_grid(self.swing_grid.load(Ordering::Relaxed)),
            self.swing_amount_percent
                .load(Ordering::Relaxed)
                .clamp(i32::from(SWING_AMOUNT_MIN), i32::from(SWING_AMOUNT_MAX)) as i16,
        )
    }

    fn gain_for_sample(&self, sample: VoiceSample) -> f32 {
        let bits = match sample {
            VoiceSample::Normal => self.normal_gain_bits.load(Ordering::Relaxed),
            VoiceSample::Accent => self.accent_gain_bits.load(Ordering::Relaxed),
            VoiceSample::Subdivision => self.subdivision_gain_bits.load(Ordering::Relaxed),
        };
        f32::from_bits(bits).clamp(0.0, 1.0)
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
    sample_rate: u32,
    lookahead_frames: u64,
    output_frame: u64,
    planning_frame: u64,
    scheduled_voices: Vec<ScheduledVoice>,
    scheduled_visuals: VecDeque<ScheduledVisual>,
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
        let current_gain = f32::from_bits(shared.boost_gain_bits.load(Ordering::Relaxed));
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
            sample_rate,
            lookahead_frames: u64::from(sample_rate) * SOURCE_OFFSET_LOOKAHEAD_MS / 1_000,
            output_frame: 0,
            planning_frame: 0,
            scheduled_voices: Vec::with_capacity(MAX_SCHEDULED_EVENTS),
            scheduled_visuals: VecDeque::with_capacity(MAX_SCHEDULED_EVENTS),
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
            self.start_scheduler();
        } else if !running && self.was_running {
            self.scheduler.stop();
            self.scheduled_voices.clear();
            self.scheduled_visuals.clear();
            for voice in &mut self.voices {
                voice.active = false;
            }
        }
        self.was_running = running;

        if running {
            if self.was_running {
                self.plan_next_frame();
            }
            self.trigger_due_voices();
            self.emit_due_visuals();
            self.output_frame = self.output_frame.saturating_add(1);
        }

        self.advance_gain();
        (self.mix_voices() * self.current_gain).clamp(-1.0, 1.0)
    }

    fn start_scheduler(&mut self) {
        self.scheduler.start();
        self.output_frame = 0;
        self.planning_frame = 0;
        self.scheduled_voices.clear();
        self.scheduled_visuals.clear();
        while self.planning_frame <= self.lookahead_frames {
            self.plan_next_frame();
        }
    }

    fn plan_next_frame(&mut self) {
        let bpm_milli = self.shared.bpm_milli.load(Ordering::Relaxed);
        let beats_per_bar = self.shared.beats_per_bar.load(Ordering::Relaxed) as u8;
        let beat_unit = self.shared.beat_unit.load(Ordering::Relaxed) as u8;
        let subdivision = self.shared.subdivision.load(Ordering::Relaxed) as u8;
        let mut beats = [None; MAX_SCHEDULED_EVENTS];
        let mut beat_count = 0;
        let mut overflowed = false;
        self.scheduler.advance_frame(
            bpm_milli,
            beats_per_bar,
            beat_unit,
            subdivision,
            self.shared.swing(),
            |beat| {
                if let Some(slot) = beats.get_mut(beat_count) {
                    *slot = Some(beat);
                    beat_count += 1;
                } else {
                    overflowed = true;
                }
            },
        );
        if overflowed {
            self.diagnostics.event_drops.fetch_add(1, Ordering::Relaxed);
        }
        for beat in beats.into_iter().take(beat_count).flatten() {
            self.schedule_event(self.planning_frame, beat);
        }
        self.planning_frame = self.planning_frame.saturating_add(1);
    }

    fn schedule_event(&mut self, rhythmic_frame: u64, beat: BeatEvent) {
        if self.scheduled_visuals.len() == MAX_SCHEDULED_EVENTS {
            self.scheduled_visuals.pop_front();
            self.diagnostics.event_drops.fetch_add(1, Ordering::Relaxed);
        }
        self.scheduled_visuals.push_back(ScheduledVisual {
            due_frame: rhythmic_frame,
            beat,
        });

        let accent_enabled = self.shared.accent_enabled.load(Ordering::Relaxed);
        let sample = voice_sample_for_beat(beat, accent_enabled);
        let offset_frames = self.effective_offset_frames(sample);
        let due_frame = if offset_frames < 0 {
            let advance = offset_frames.unsigned_abs();
            if advance >= rhythmic_frame {
                // Keep the first downbeat immediate, but do not collapse the
                // rest of the negative pre-roll into a burst at frame zero.
                if rhythmic_frame == 0 {
                    0
                } else {
                    return;
                }
            } else {
                rhythmic_frame - advance
            }
        } else {
            rhythmic_frame.saturating_add(offset_frames as u64)
        };

        if self.scheduled_voices.len() == MAX_SCHEDULED_EVENTS {
            self.diagnostics.voice_drops.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let insert_at = self
            .scheduled_voices
            .partition_point(|scheduled| scheduled.due_frame <= due_frame);
        self.scheduled_voices
            .insert(insert_at, ScheduledVoice { due_frame, sample });
    }

    fn effective_offset_frames(&self, sample: VoiceSample) -> i64 {
        let (manual_ms, auto_align) = self.shared.timing_for_sample(sample);
        let manual_frames = i64::from(manual_ms) * i64::from(self.sample_rate) / 1_000;
        let transient_frames = if auto_align {
            let analysis = self.transient_for_sample(sample);
            if analysis.reliable {
                analysis
                    .frame
                    .and_then(|frame| i64::try_from(frame).ok())
                    .unwrap_or(0)
            } else {
                0
            }
        } else {
            0
        };
        (manual_frames - transient_frames).clamp(
            -(self.lookahead_frames as i64),
            self.lookahead_frames as i64,
        )
    }

    fn transient_for_sample(&self, sample: VoiceSample) -> TransientAnalysis {
        match sample {
            VoiceSample::Normal => self.sample_bank.normal_transient,
            VoiceSample::Accent => self.sample_bank.accent_transient,
            VoiceSample::Subdivision => self.sample_bank.subdivision_transient,
        }
    }

    fn trigger_due_voices(&mut self) {
        while self
            .scheduled_voices
            .first()
            .is_some_and(|scheduled| scheduled.due_frame <= self.output_frame)
        {
            let scheduled = self.scheduled_voices.remove(0);
            self.trigger_voice(scheduled.sample);
        }
    }

    fn emit_due_visuals(&mut self) {
        while self
            .scheduled_visuals
            .front()
            .is_some_and(|scheduled| scheduled.due_frame <= self.output_frame)
        {
            let scheduled = self
                .scheduled_visuals
                .pop_front()
                .expect("front was checked");
            if self
                .event_producer
                .push(AudioEvent {
                    beat: scheduled.beat,
                })
                .is_err()
            {
                self.diagnostics.event_drops.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn trigger_voice(&mut self, sample: VoiceSample) {
        if let Some(voice) = self.voices.iter_mut().find(|voice| !voice.active) {
            voice.active = true;
            voice.position = 0;
            voice.sample = sample;
            voice.gain = self.shared.gain_for_sample(voice.sample);
        } else {
            self.diagnostics.voice_drops.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn mix_voices(&mut self) -> f32 {
        let mut mixed = 0.0;
        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            let samples = match voice.sample {
                VoiceSample::Normal => &self.sample_bank.normal,
                VoiceSample::Accent => &self.sample_bank.accent,
                VoiceSample::Subdivision => &self.sample_bank.subdivision,
            };

            if let Some(sample) = samples.get(voice.position) {
                mixed += *sample * voice.gain;
                voice.position += 1;
            } else {
                voice.active = false;
            }
        }
        mixed
    }

    fn advance_gain(&mut self) {
        let target = f32::from_bits(self.shared.boost_gain_bits.load(Ordering::Relaxed))
            .clamp(0.0, db_to_gain(OUTPUT_VOLUME_DB_MAX));
        if (self.current_gain - target).abs() <= self.gain_step {
            self.current_gain = target;
        } else if self.current_gain < target {
            self.current_gain += self.gain_step;
        } else {
            self.current_gain -= self.gain_step;
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ScheduledVoice {
    due_frame: u64,
    sample: VoiceSample,
}

#[derive(Clone, Copy, Debug)]
struct ScheduledVisual {
    due_frame: u64,
    beat: BeatEvent,
}

#[derive(Clone, Copy, Debug, Default)]
struct Voice {
    active: bool,
    position: usize,
    sample: VoiceSample,
    gain: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum VoiceSample {
    #[default]
    Normal,
    Accent,
    Subdivision,
}

fn voice_sample_for_beat(beat: BeatEvent, accent_enabled: bool) -> VoiceSample {
    if beat.is_accent && accent_enabled {
        VoiceSample::Accent
    } else if beat.subdivision_index > 0 {
        VoiceSample::Subdivision
    } else {
        VoiceSample::Normal
    }
}

fn swing_grid_code(grid: ConfigSwingGrid) -> u32 {
    match grid {
        ConfigSwingGrid::Quarter => 4,
        ConfigSwingGrid::Eighth => 8,
        ConfigSwingGrid::Sixteenth => 16,
    }
}

fn scheduler_swing_grid(code: u32) -> SchedulerSwingGrid {
    match code {
        4 => SchedulerSwingGrid::Quarter,
        16 => SchedulerSwingGrid::Sixteenth,
        _ => SchedulerSwingGrid::Eighth,
    }
}

fn percent_gain(volume_percent: u8) -> f32 {
    f32::from(volume_percent.min(100)) / 100.0
}

fn output_volume_gain(db: f32) -> f32 {
    let db = db.clamp(OUTPUT_VOLUME_DB_MIN, OUTPUT_VOLUME_DB_MAX);
    if db <= OUTPUT_VOLUME_DB_MIN {
        0.0
    } else {
        db_to_gain(db)
    }
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
    use std::sync::Arc;

    use rtrb::RingBuffer;

    use super::{
        AudioEvent, AudioShared, DiagnosticsShared, RenderState, VoiceSample, db_to_gain,
        output_volume_gain, percent_gain, voice_sample_for_beat,
    };
    use crate::audio::BeatEvent;
    use crate::audio::sample_bank::{SampleBank, TransientAnalysis};
    use crate::config::{AppConfig, SoundConfig, SoundTimingSettings, SwingGrid};

    fn fast_config() -> AppConfig {
        AppConfig {
            bpm_milli: 600_000,
            sound: SoundConfig {
                accent_enabled: false,
                ..SoundConfig::default()
            },
            ..AppConfig::default()
        }
    }

    fn render_state(config: &AppConfig, normal_transient: TransientAnalysis) -> RenderState {
        let (producer, _consumer) = RingBuffer::<AudioEvent>::new(256);
        RenderState::new(
            48_000,
            2,
            Arc::new(AudioShared::from_config(config)),
            Arc::new(DiagnosticsShared::default()),
            producer,
            SampleBank {
                normal: vec![1.0],
                accent: vec![1.0],
                subdivision: vec![1.0],
                normal_transient,
                accent_transient: TransientAnalysis::default(),
                subdivision_transient: TransientAnalysis::default(),
            },
        )
    }

    #[test]
    fn sound_percent_and_output_volume_are_independent_gain_stages() {
        let gain = db_to_gain(12.0);
        assert!((gain - 3.981_071_7).abs() < 0.0001);
        assert!((percent_gain(50) * gain - gain * 0.5).abs() < 0.0001);
        assert_eq!(output_volume_gain(-60.0), 0.0);
        assert!((output_volume_gain(-6.0) - 0.501_187_2).abs() < 0.0001);
    }

    #[test]
    fn accent_can_fall_back_to_the_normal_click() {
        let accent = BeatEvent {
            beat_index: 0,
            beats_per_bar: 4,
            subdivision_index: 0,
            subdivisions_per_beat: 4,
            is_accent: true,
        };
        assert_eq!(voice_sample_for_beat(accent, true), VoiceSample::Accent);
        assert_eq!(voice_sample_for_beat(accent, false), VoiceSample::Normal);

        let subdivision = BeatEvent {
            subdivision_index: 1,
            is_accent: false,
            ..accent
        };
        assert_eq!(
            voice_sample_for_beat(subdivision, false),
            VoiceSample::Subdivision
        );
    }

    #[test]
    fn source_offset_moves_audio_without_moving_visual_events() {
        let mut config = fast_config();
        let source = config.sound.normal_source_id();
        config.sound.set_timing_for(
            source,
            SoundTimingSettings {
                manual_offset_ms: 100,
                auto_align_transient: false,
            },
        );
        let mut state = render_state(&config, TransientAnalysis::default());

        state.start_scheduler();

        assert_eq!(state.scheduled_visuals.front().unwrap().due_frame, 0);
        assert_eq!(state.scheduled_voices.first().unwrap().due_frame, 4_800);
    }

    #[test]
    fn negative_preroll_does_not_stack_future_clicks_at_start() {
        let mut config = fast_config();
        let source = config.sound.normal_source_id();
        config.sound.set_timing_for(
            source,
            SoundTimingSettings {
                manual_offset_ms: -100,
                auto_align_transient: false,
            },
        );
        let mut state = render_state(&config, TransientAnalysis::default());

        state.start_scheduler();

        assert_eq!(
            state
                .scheduled_voices
                .iter()
                .filter(|scheduled| scheduled.due_frame == 0)
                .count(),
            1
        );
    }

    #[test]
    fn automatic_alignment_subtracts_detected_transient_position() {
        let mut config = fast_config();
        let source = config.sound.normal_source_id();
        config.sound.set_timing_for(
            source,
            SoundTimingSettings {
                manual_offset_ms: 0,
                auto_align_transient: true,
            },
        );
        let mut state = render_state(
            &config,
            TransientAnalysis {
                frame: Some(1_440),
                milliseconds: Some(30.0),
                reliable: true,
            },
        );

        state.start_scheduler();

        assert!(
            state
                .scheduled_voices
                .iter()
                .any(|scheduled| scheduled.due_frame == 3_360)
        );
    }

    #[test]
    fn fully_swung_clicks_are_scheduled_on_the_same_audio_frame() {
        let mut config = fast_config();
        config.audio.subdivision = 2;
        config.audio.swing.grid = SwingGrid::Eighth;
        config.audio.swing.amount_percent = 100;
        let mut state = render_state(&config, TransientAnalysis::default());

        state.start_scheduler();

        assert_eq!(
            state
                .scheduled_voices
                .iter()
                .filter(|scheduled| scheduled.due_frame == 4_800)
                .count(),
            2
        );
        assert_eq!(
            state
                .scheduled_visuals
                .iter()
                .filter(|scheduled| scheduled.due_frame == 4_800)
                .count(),
            2
        );
    }
}
