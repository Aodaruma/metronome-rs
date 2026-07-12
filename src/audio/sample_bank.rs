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

use crate::config::AppConfig;

#[derive(Debug, Clone)]
pub struct SampleBank {
    pub normal: Vec<f32>,
    pub accent: Vec<f32>,
    pub subdivision: Vec<f32>,
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

const NORMAL_WAV: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/SIN 2.wav"));
const ACCENT_WAV: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/SIN 1.wav"));

impl SampleBank {
    pub fn from_config(
        config: &AppConfig,
        target_sample_rate: u32,
    ) -> Result<Self, SampleBankError> {
        let builtin_normal = decode_wav_mono(NORMAL_WAV)?;
        let builtin_accent = decode_wav_mono(ACCENT_WAV)?;

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
            .unwrap_or_else(|| normal.clone());

        Ok(Self {
            normal: prepare_sample(normal, target_sample_rate),
            accent: prepare_sample(accent, target_sample_rate),
            subdivision: prepare_sample(subdivision, target_sample_rate),
        })
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

fn prepare_sample(decoded: DecodedSample, target_sample_rate: u32) -> Vec<f32> {
    let mut samples = if decoded.sample_rate == target_sample_rate {
        decoded.samples
    } else {
        resample_linear(&decoded.samples, decoded.sample_rate, target_sample_rate)
    };

    trim_to_two_seconds(&mut samples, target_sample_rate);
    apply_fade_out(&mut samples, target_sample_rate);
    normalize_peak(&mut samples, 0.85);
    samples
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

    use super::{SampleBank, validate_audio_file};
    use crate::config::AppConfig;

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

        config.sound.subdivision_path =
            Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/SIN 1.wav"));
        let custom_bank = SampleBank::from_config(&config, 44_100).expect("samples should load");
        assert_ne!(custom_bank.subdivision, custom_bank.normal);
    }
}
