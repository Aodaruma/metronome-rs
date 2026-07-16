use std::fs::File;
use std::io::{self, Cursor};
use std::path::Path;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use thiserror::Error;

use crate::config::{AppConfig, BuiltinSound};

#[derive(Debug, Clone)]
pub struct SampleBank {
    pub normal: Vec<f32>,
    pub accent: Vec<f32>,
    pub subdivision: Vec<f32>,
    pub normal_transient: TransientAnalysis,
    pub accent_transient: TransientAnalysis,
    pub subdivision_transient: TransientAnalysis,
}

/// A reliable estimate of the first audible transient in a prepared sample.
///
/// Analysis is performed while constructing the sample bank, after resampling
/// and before trimming, fading, or normalization. An unreliable result does
/// not expose a position so it cannot accidentally be used as an automatic
/// timing correction.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TransientAnalysis {
    pub frame: Option<usize>,
    pub milliseconds: Option<f32>,
    pub reliable: bool,
}

impl TransientAnalysis {
    const fn unreliable() -> Self {
        Self {
            frame: None,
            milliseconds: None,
            reliable: false,
        }
    }
}

#[derive(Debug, Error)]
pub enum SampleBankError {
    #[error("WAVを読み込めませんでした: {0}")]
    Hound(#[from] hound::Error),
    #[error("音源が空です")]
    Empty,
    #[error("未対応のWAV形式です")]
    Unsupported,
    #[error("音源ファイルを読み込めませんでした: {0}")]
    Io(#[from] io::Error),
    #[error("音源ファイルをデコードできませんでした: {0}")]
    Symphonia(#[from] SymphoniaError),
    #[error("音声トラックが見つかりません")]
    NoAudioTrack,
    #[error("音声コーデック情報がありません")]
    MissingCodecParameters,
    #[error("音源ファイルは50 MiB以下を指定してください")]
    FileTooLarge,
}

const SIN_1_WAV: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/SIN 1.wav"));
const SIN_2_WAV: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/SIN 2.wav"));
const SIN_3_WAV: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/SIN 3.wav"));
const SIN_4_WAV: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/SIN 4.wav"));

impl SampleBank {
    pub fn from_config(
        config: &AppConfig,
        target_sample_rate: u32,
    ) -> Result<Self, SampleBankError> {
        let builtin_normal = decode_wav_mono(builtin_wav(config.sound.normal_builtin))?;
        let builtin_accent = decode_wav_mono(builtin_wav(config.sound.accent_builtin))?;

        let normal = config
            .sound
            .normal_path
            .as_deref()
            .and_then(|path| decode_audio_file(path).ok())
            .unwrap_or(builtin_normal);
        let accent = config
            .sound
            .accent_path
            .as_deref()
            .and_then(|path| decode_audio_file(path).ok())
            .unwrap_or(builtin_accent);
        let subdivision = config
            .sound
            .subdivision_path
            .as_deref()
            .and_then(|path| decode_audio_file(path).ok())
            .or_else(|| {
                config
                    .sound
                    .subdivision_builtin
                    .and_then(|sound| decode_wav_mono(builtin_wav(sound)).ok())
            })
            .unwrap_or_else(|| normal.clone());

        let normal = prepare_sample(normal, target_sample_rate);
        let accent = prepare_sample(accent, target_sample_rate);
        let subdivision = prepare_sample(subdivision, target_sample_rate);

        Ok(Self {
            normal: normal.samples,
            accent: accent.samples,
            subdivision: subdivision.samples,
            normal_transient: normal.transient,
            accent_transient: accent.transient,
            subdivision_transient: subdivision.transient,
        })
    }
}

fn builtin_wav(sound: BuiltinSound) -> &'static [u8] {
    match sound {
        BuiltinSound::Sin1 => SIN_1_WAV,
        BuiltinSound::Sin2 => SIN_2_WAV,
        BuiltinSound::Sin3 => SIN_3_WAV,
        BuiltinSound::Sin4 => SIN_4_WAV,
    }
}

pub fn validate_audio_file(path: &Path) -> Result<(), SampleBankError> {
    decode_audio_file(path).map(|_| ())
}

fn decode_audio_file(path: &Path) -> Result<DecodedSample, SampleBankError> {
    const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
    if path.metadata()?.len() > MAX_FILE_SIZE {
        return Err(SampleBankError::FileTooLarge);
    }

    let source = Box::new(File::open(path)?);
    let media_source = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }

    let mut format = symphonia::default::get_probe().probe(
        &hint,
        media_source,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or(SampleBankError::NoAudioTrack)?;
    let codec_parameters = track
        .codec_params
        .as_ref()
        .ok_or(SampleBankError::MissingCodecParameters)?
        .audio()
        .ok_or(SampleBankError::MissingCodecParameters)?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_parameters, &AudioDecoderOptions::default())?;
    let track_id = track.id;
    let mut sample_rate = 0;
    let mut mono = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => break,
            Err(error) => return Err(error.into()),
        };
        if packet.track_id != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) | Err(SymphoniaError::IoError(_)) => continue,
            Err(error) => return Err(error.into()),
        };
        sample_rate = decoded.spec().rate();
        let channels = decoded.spec().channels().count().max(1);
        let mut interleaved = vec![0.0_f32; decoded.samples_interleaved()];
        decoded.copy_to_slice_interleaved(&mut interleaved);
        for frame in interleaved.chunks(channels) {
            mono.push(frame.iter().copied().sum::<f32>() / frame.len() as f32);
        }

        if sample_rate > 0 && mono.len() >= sample_rate as usize * 5 {
            break;
        }
    }

    if sample_rate == 0 || mono.is_empty() {
        return Err(SampleBankError::Empty);
    }

    Ok(DecodedSample {
        sample_rate,
        samples: mono,
    })
}

#[derive(Clone)]
struct DecodedSample {
    sample_rate: u32,
    samples: Vec<f32>,
}

fn decode_wav_mono(bytes: &[u8]) -> Result<DecodedSample, SampleBankError> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes))?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));

    let raw = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|sample| sample.clamp(-1.0, 1.0))
            .collect::<Vec<_>>(),
        hound::SampleFormat::Int => match spec.bits_per_sample {
            8 => reader
                .samples::<i8>()
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|sample| f32::from(sample) / 128.0)
                .collect::<Vec<_>>(),
            16 => reader
                .samples::<i16>()
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|sample| f32::from(sample) / 32768.0)
                .collect::<Vec<_>>(),
            24 | 32 => {
                let max = (1_i64 << (u32::from(spec.bits_per_sample) - 1)) as f32;
                reader
                    .samples::<i32>()
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|sample| (sample as f32 / max).clamp(-1.0, 1.0))
                    .collect::<Vec<_>>()
            }
            _ => return Err(SampleBankError::Unsupported),
        },
    };

    if raw.is_empty() {
        return Err(SampleBankError::Empty);
    }

    let mut mono = Vec::with_capacity(raw.len() / channels + 1);
    for frame in raw.chunks(channels) {
        let sum = frame.iter().copied().sum::<f32>();
        mono.push(sum / frame.len() as f32);
    }

    Ok(DecodedSample {
        sample_rate: spec.sample_rate,
        samples: mono,
    })
}

struct PreparedSample {
    samples: Vec<f32>,
    transient: TransientAnalysis,
}

fn prepare_sample(decoded: DecodedSample, target_sample_rate: u32) -> PreparedSample {
    let mut samples = if decoded.sample_rate == target_sample_rate {
        decoded.samples
    } else {
        resample_linear(&decoded.samples, decoded.sample_rate, target_sample_rate)
    };

    let transient = analyze_first_transient(&samples, target_sample_rate);
    trim_to_two_seconds(&mut samples, target_sample_rate);
    apply_fade_out(&mut samples, target_sample_rate);
    normalize_peak(&mut samples, 0.85);
    PreparedSample { samples, transient }
}

/// Finds the first significant short attack in the first 500 ms of a sample.
///
/// A one-millisecond peak envelope retains click-like attacks without making a
/// later, louder peak the timing reference. Reliability requires both enough
/// contrast over the envelope's background floor and a sufficiently steep
/// rise over a short interval. This deliberately rejects silence, steady
/// noise, and slow fades, which are unsafe inputs for automatic correction.
fn analyze_first_transient(samples: &[f32], sample_rate: u32) -> TransientAnalysis {
    const ANALYSIS_MILLISECONDS: usize = 500;
    const MIN_SIGNAL_PEAK: f32 = 1.0e-5;
    const MIN_PEAK_TO_FLOOR_RATIO: f32 = 2.5;
    const MIN_DYNAMIC_RANGE_FRACTION: f32 = 0.2;
    const ONSET_THRESHOLD_FRACTION: f32 = 0.1;
    const MIN_ATTACK_RISE_FRACTION: f32 = 0.15;
    const ATTACK_MILLISECONDS: usize = 8;

    if samples.is_empty() || sample_rate == 0 {
        return TransientAnalysis::unreliable();
    }

    let analysis_len = samples.len().min(
        (sample_rate as usize)
            .saturating_mul(ANALYSIS_MILLISECONDS)
            .div_ceil(1_000),
    );
    let envelope_frame_len = (sample_rate as usize / 1_000).max(1);
    let envelope = samples[..analysis_len]
        .chunks(envelope_frame_len)
        .map(|frame| {
            frame
                .iter()
                .map(|sample| sample.abs())
                .fold(0.0_f32, f32::max)
        })
        .collect::<Vec<_>>();

    if envelope.is_empty() {
        return TransientAnalysis::unreliable();
    }

    let signal_peak = envelope.iter().copied().fold(0.0_f32, f32::max);
    if !signal_peak.is_finite() || signal_peak < MIN_SIGNAL_PEAK {
        return TransientAnalysis::unreliable();
    }

    let mut sorted_envelope = envelope.clone();
    sorted_envelope.sort_by(f32::total_cmp);
    let floor_index = (sorted_envelope.len() - 1) / 5;
    let background_floor = sorted_envelope[floor_index];
    let dynamic_range = signal_peak - background_floor;

    let has_dynamic_range = dynamic_range >= signal_peak * MIN_DYNAMIC_RANGE_FRACTION;
    let has_background_contrast = background_floor <= f32::EPSILON
        || signal_peak >= background_floor * MIN_PEAK_TO_FLOOR_RATIO;
    if !has_dynamic_range || !has_background_contrast {
        return TransientAnalysis::unreliable();
    }

    let onset_threshold = background_floor + dynamic_range * ONSET_THRESHOLD_FRACTION;
    let minimum_attack_rise = dynamic_range * MIN_ATTACK_RISE_FRACTION;
    let attack_frames = ATTACK_MILLISECONDS.max(1);
    let onset_envelope_frame = envelope.iter().enumerate().find_map(|(index, &level)| {
        if level < onset_threshold {
            return None;
        }

        let earlier_level = index
            .checked_sub(attack_frames)
            .and_then(|earlier| envelope.get(earlier).copied())
            .unwrap_or(background_floor);
        (level - earlier_level >= minimum_attack_rise).then_some(index)
    });
    let Some(onset_envelope_frame) = onset_envelope_frame else {
        return TransientAnalysis::unreliable();
    };

    let envelope_start = onset_envelope_frame * envelope_frame_len;
    let envelope_end = (envelope_start + envelope_frame_len).min(analysis_len);
    let sample_threshold = background_floor + dynamic_range * ONSET_THRESHOLD_FRACTION;
    let frame = samples[envelope_start..envelope_end]
        .iter()
        .position(|sample| sample.abs() >= sample_threshold)
        .map(|within_envelope| envelope_start + within_envelope)
        .unwrap_or(envelope_start);

    TransientAnalysis {
        frame: Some(frame),
        milliseconds: Some(frame as f32 * 1_000.0 / sample_rate as f32),
        reliable: true,
    }
}

fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if input.is_empty() || from_rate == 0 || to_rate == 0 {
        return Vec::new();
    }

    let out_len = ((input.len() as u64 * u64::from(to_rate)) / u64::from(from_rate)).max(1);
    let ratio = f64::from(from_rate) / f64::from(to_rate);
    let mut output = Vec::with_capacity(out_len as usize);

    for out_index in 0..out_len {
        let src = out_index as f64 * ratio;
        let base = src.floor() as usize;
        let frac = (src - base as f64) as f32;
        let a = input.get(base).copied().unwrap_or(0.0);
        let b = input.get(base + 1).copied().unwrap_or(a);
        output.push(a + (b - a) * frac);
    }

    output
}

fn trim_to_two_seconds(samples: &mut Vec<f32>, sample_rate: u32) {
    let max_len = sample_rate as usize * 2;
    if samples.len() > max_len {
        samples.truncate(max_len);
    }
}

fn apply_fade_out(samples: &mut [f32], sample_rate: u32) {
    let fade_len = ((sample_rate as f32 * 0.005) as usize).min(samples.len());
    if fade_len == 0 {
        return;
    }

    let start = samples.len() - fade_len;
    for (index, sample) in samples[start..].iter_mut().enumerate() {
        let gain = 1.0 - (index as f32 / fade_len as f32);
        *sample *= gain;
    }
}

fn normalize_peak(samples: &mut [f32], target_peak: f32) {
    let peak = samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max);
    if peak <= f32::EPSILON {
        return;
    }

    let gain = target_peak / peak;
    for sample in samples {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        DecodedSample, SampleBank, analyze_first_transient, builtin_wav, decode_wav_mono,
        prepare_sample, validate_audio_file,
    };
    use crate::config::{AppConfig, BuiltinSound};

    #[test]
    fn embedded_wav_is_accepted_as_a_custom_sound() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/SIN 1.wav");
        validate_audio_file(&path).expect("embedded WAV should decode");
    }

    #[test]
    fn subdivision_uses_normal_sound_until_a_custom_source_is_selected() {
        let mut config = AppConfig::default();
        let default_bank = SampleBank::from_config(&config, 44_100).expect("samples should load");
        assert_eq!(default_bank.subdivision, default_bank.normal);

        config.sound.subdivision_builtin = Some(BuiltinSound::Sin1);
        let builtin_bank = SampleBank::from_config(&config, 44_100).expect("samples should load");
        assert_ne!(builtin_bank.subdivision, builtin_bank.normal);

        config.sound.subdivision_builtin = None;
        config.sound.subdivision_path =
            Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/SIN 1.wav"));
        let custom_bank = SampleBank::from_config(&config, 44_100).expect("samples should load");
        assert_ne!(custom_bank.subdivision, custom_bank.normal);
    }

    #[test]
    fn every_builtin_sound_decodes() {
        for sound in [
            BuiltinSound::Sin1,
            BuiltinSound::Sin2,
            BuiltinSound::Sin3,
            BuiltinSound::Sin4,
        ] {
            let decoded = decode_wav_mono(builtin_wav(sound)).expect("built-in WAV should decode");
            assert!(!decoded.samples.is_empty());
        }
    }

    #[test]
    fn silence_has_no_reliable_transient() {
        let analysis = analyze_first_transient(&vec![0.0; 24_000], 48_000);

        assert!(!analysis.reliable);
        assert_eq!(analysis.frame, None);
        assert_eq!(analysis.milliseconds, None);
    }

    #[test]
    fn stationary_noise_has_no_reliable_transient() {
        let mut state = 0x1234_5678_u32;
        let noise = (0..24_000)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let unit = (state >> 8) as f32 / 0x00ff_ffff_u32 as f32;
                (unit * 2.0 - 1.0) * 0.02
            })
            .collect::<Vec<_>>();

        let analysis = analyze_first_transient(&noise, 48_000);

        assert!(!analysis.reliable);
        assert_eq!(analysis.frame, None);
        assert_eq!(analysis.milliseconds, None);
    }

    #[test]
    fn slow_attack_has_no_reliable_transient() {
        let sample_rate = 48_000;
        let attack_frames = sample_rate * 300 / 1_000;
        let samples = (0..sample_rate / 2)
            .map(|frame| (frame as f32 / attack_frames as f32).min(1.0))
            .collect::<Vec<_>>();

        let analysis = analyze_first_transient(&samples, sample_rate as u32);

        assert!(!analysis.reliable);
        assert_eq!(analysis.frame, None);
        assert_eq!(analysis.milliseconds, None);
    }

    #[test]
    fn leading_silence_then_sharp_peak_reports_first_transient() {
        let sample_rate = 48_000_u32;
        let onset_frame = sample_rate as usize * 120 / 1_000;
        let mut samples = vec![0.0; sample_rate as usize / 2];
        samples[onset_frame] = 0.2;
        samples[onset_frame + 1] = 0.15;
        samples[onset_frame + 2] = 0.1;
        // A later and larger-width event must not become the timing reference.
        let later_frame = sample_rate as usize * 300 / 1_000;
        samples[later_frame..later_frame + 48].fill(1.0);

        let analysis = analyze_first_transient(&samples, sample_rate);

        assert!(analysis.reliable);
        assert_eq!(analysis.frame, Some(onset_frame));
        assert_eq!(analysis.milliseconds, Some(120.0));
    }

    #[test]
    fn preparation_analyzes_after_resampling_and_before_normalization() {
        let source_rate = 24_000_u32;
        let target_rate = 48_000_u32;
        let onset_frame = source_rate as usize / 10;
        let mut samples = vec![0.0; source_rate as usize / 2];
        samples[onset_frame] = 0.02;
        samples[onset_frame + 1] = 0.01;

        let prepared = prepare_sample(
            DecodedSample {
                sample_rate: source_rate,
                samples,
            },
            target_rate,
        );

        assert!(prepared.transient.reliable);
        let detected_frame = prepared.transient.frame.expect("transient frame");
        assert!(detected_frame.abs_diff(onset_frame * 2) <= 1);
        let detected_ms = prepared
            .transient
            .milliseconds
            .expect("transient milliseconds");
        assert!((detected_ms - 100.0).abs() < 0.03);
        let prepared_peak = prepared
            .samples
            .iter()
            .copied()
            .map(f32::abs)
            .fold(0.0_f32, f32::max);
        assert!((prepared_peak - 0.85).abs() < f32::EPSILON * 4.0);
    }
}
