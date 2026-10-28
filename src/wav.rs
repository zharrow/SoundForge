//! Lecture et écriture WAV.
//! Écriture : PCM 16 bits mono, le format conseillé par Godot pour les SFX.
//! Lecture : PCM 16/24 bits ou flottant 32 bits, mono ou multicanal (mixé en mono).

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

const BITS_PER_SAMPLE: u16 = 16;
const CHANNELS: u16 = 1;
const HEADER_SIZE: u32 = 36;
const FORMAT_PCM: u16 = 1;
const FORMAT_FLOAT: u16 = 3;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;

pub fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> io::Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    write_header(&mut writer, samples.len() as u32, sample_rate)?;
    for &sample in samples {
        writer.write_all(&to_pcm16(sample).to_le_bytes())?;
    }
    writer.flush()
}

/// WAV stéréo 16 bits (musique) : canaux entrelacés.
pub fn encode_wav_stereo(left: &[f32], right: &[f32], sample_rate: u32) -> Vec<u8> {
    let frames = left.len().min(right.len());
    let data_size = (frames * 4) as u32;
    let mut bytes = Vec::with_capacity(44 + frames * 4);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(HEADER_SIZE + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&FORMAT_PCM.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 4).to_le_bytes());
    bytes.extend_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    for i in 0..frames {
        bytes.extend_from_slice(&to_pcm16(left[i]).to_le_bytes());
        bytes.extend_from_slice(&to_pcm16(right[i]).to_le_bytes());
    }
    bytes
}

/// Le même fichier qu'`write_wav`, en mémoire : l'éditeur l'écoute sans passer par le disque.
pub fn encode_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER_SIZE as usize + 8 + samples.len() * 2);
    write_header(&mut bytes, samples.len() as u32, sample_rate).expect("écriture en mémoire");
    for &sample in samples {
        bytes.extend_from_slice(&to_pcm16(sample).to_le_bytes());
    }
    bytes
}

fn write_header(writer: &mut impl Write, sample_count: u32, sample_rate: u32) -> io::Result<()> {
    let block_align = CHANNELS * BITS_PER_SAMPLE / 8;
    let data_size = sample_count * u32::from(block_align);

    writer.write_all(b"RIFF")?;
    writer.write_all(&(HEADER_SIZE + data_size).to_le_bytes())?;
    writer.write_all(b"WAVEfmt ")?;
    writer.write_all(&16_u32.to_le_bytes())?;
    writer.write_all(&FORMAT_PCM.to_le_bytes())?;
    writer.write_all(&CHANNELS.to_le_bytes())?;
    writer.write_all(&sample_rate.to_le_bytes())?;
    writer.write_all(&(sample_rate * u32::from(block_align)).to_le_bytes())?;
    writer.write_all(&block_align.to_le_bytes())?;
    writer.write_all(&BITS_PER_SAMPLE.to_le_bytes())?;
    writer.write_all(b"data")?;
    writer.write_all(&data_size.to_le_bytes())
}

fn to_pcm16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

// ---------- Lecture ----------

#[derive(Debug, Clone, Copy, PartialEq)]
struct Format {
    code: u16,
    channels: u16,
    sample_rate: u32,
    bits: u16,
}

/// Lit un WAV et renvoie (échantillons mono entre -1 et 1, fréquence d'échantillonnage).
pub fn read_wav(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{} : {e}", path.display()))?;
    parse_wav(&bytes)
}

pub fn parse_wav(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("Fichier WAV invalide (en-tête RIFF/WAVE absent).".into());
    }
    let mut format = None;
    for (id, body) in chunks(&bytes[12..]) {
        match id {
            b"fmt " => format = Some(parse_format(body)?),
            b"data" => {
                let format = format.ok_or("Bloc 'data' trouvé avant le bloc 'fmt '.")?;
                return Ok((decode_samples(body, format)?, format.sample_rate));
            }
            _ => {}
        }
    }
    Err("Aucun bloc audio 'data' dans le fichier.".into())
}

fn chunks(mut bytes: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    std::iter::from_fn(move || {
        if bytes.len() < 8 {
            return None;
        }
        let id = &bytes[0..4];
        let size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        let end = (8 + size).min(bytes.len());
        let body = &bytes[8..end];
        let padded = (end + (size & 1)).min(bytes.len());
        bytes = &bytes[padded..];
        Some((id, body))
    })
}

fn parse_format(body: &[u8]) -> Result<Format, String> {
    if body.len() < 16 {
        return Err("Bloc 'fmt ' trop court.".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([body[i], body[i + 1]]);
    let mut code = u16_at(0);
    if code == FORMAT_EXTENSIBLE && body.len() >= 26 {
        code = u16_at(24);
    }
    Ok(Format {
        code,
        channels: u16_at(2).max(1),
        sample_rate: u32::from_le_bytes([body[4], body[5], body[6], body[7]]),
        bits: u16_at(14),
    })
}

fn decode_samples(data: &[u8], format: Format) -> Result<Vec<f32>, String> {
    let decode: fn(&[u8]) -> f32 = match (format.code, format.bits) {
        (FORMAT_PCM, 16) => |b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0,
        (FORMAT_PCM, 24) => {
            |b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0
        }
        (FORMAT_FLOAT, 32) => |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        (code, bits) => {
            return Err(format!(
                "Format WAV non pris en charge (code {code}, {bits} bits)."
            ));
        }
    };
    let width = usize::from(format.bits / 8);
    let channels = usize::from(format.channels);
    let frame_bytes = width * channels;
    Ok(data
        .chunks_exact(frame_bytes)
        .map(|frame| frame.chunks_exact(width).map(decode).sum::<f32>() / channels as f32)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_44_bytes_and_sizes_match() {
        let mut bytes = Vec::new();
        write_header(&mut bytes, 100, 44_100).unwrap();
        assert_eq!(bytes.len(), 44);
        assert_eq!(&bytes[40..44], &200_u32.to_le_bytes());
        assert_eq!(&bytes[4..8], &236_u32.to_le_bytes());
    }

    #[test]
    fn pcm_conversion_clamps() {
        assert_eq!(to_pcm16(2.0), i16::MAX);
        assert_eq!(to_pcm16(-2.0), -i16::MAX);
        assert_eq!(to_pcm16(0.0), 0);
    }

    #[test]
    fn write_then_read_round_trip() {
        let path = std::env::temp_dir().join("sound_forge_roundtrip.wav");
        let samples = [0.0, 0.5, -0.5, 0.25];
        write_wav(&path, &samples, 22_050).unwrap();
        let (read, rate) = read_wav(&path).unwrap();
        assert_eq!(rate, 22_050);
        assert!(read.iter().zip(samples).all(|(a, b)| (a - b).abs() < 1e-3));
    }

    #[test]
    fn stereo_float_is_mixed_to_mono() {
        let mut bytes = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        bytes.extend(16_u32.to_le_bytes());
        for value in [FORMAT_FLOAT, 2] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(48_000_u32.to_le_bytes());
        bytes.extend(384_000_u32.to_le_bytes());
        bytes.extend(8_u16.to_le_bytes());
        bytes.extend(32_u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(8_u32.to_le_bytes());
        bytes.extend(1.0_f32.to_le_bytes());
        bytes.extend(0.0_f32.to_le_bytes());
        let (samples, rate) = parse_wav(&bytes).unwrap();
        assert_eq!((samples, rate), (vec![0.5], 48_000));
    }

    #[test]
    fn rejects_non_wav() {
        assert!(parse_wav(b"not a wav file").is_err());
    }
}
