//! Bounded RIFF parser. No native file parser, pathname or extension authority.
use hair::Result;
use std::sync::atomic::{AtomicBool, Ordering};
pub const ENCODED_CAP: usize = 4 * 1024 * 1024;
pub const PCM_CAP: usize = 4 * 1024 * 1024;
const CHUNK_CAP: usize = 1024;
#[derive(Debug)]
pub struct Pcm {
    pub samples: Vec<i16>,
    pub channels: u32,
    pub rate: u32,
    pub frames: usize,
    pub duration: f64,
}
pub fn signature(b: &[u8]) -> bool {
    b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WAVE"
}
fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
pub fn decode(b: &[u8], cancel: &AtomicBool) -> Result<Pcm> {
    if cancel.load(Ordering::Acquire) {
        return Err("audio cancelled".into());
    }
    if b.len() > ENCODED_CAP
        || !signature(b)
        || usize::try_from(u32le(b, 4))
            .ok()
            .and_then(|n| n.checked_add(8))
            != Some(b.len())
    {
        return Err("WAV extent/signature/4 MiB cap refused".into());
    }
    let mut at = 12usize;
    let mut fmt = None;
    let mut data = None;
    let mut chunks = 0usize;
    while at < b.len() {
        chunks += 1;
        if chunks > CHUNK_CAP || cancel.load(Ordering::Acquire) {
            return Err("WAV work budget/cancelled".into());
        }
        let start = at.checked_add(8).ok_or("WAV offset overflow")?;
        if start > b.len() {
            return Err("WAV truncated chunk".into());
        }
        let n = usize::try_from(u32le(b, at + 4)).map_err(|_| "WAV size overflow")?;
        let end = start.checked_add(n).ok_or("WAV chunk overflow")?;
        let next = end.checked_add(n & 1).ok_or("WAV padding overflow")?;
        if next > b.len() {
            return Err("WAV chunk/padding beyond RIFF".into());
        }
        match &b[at..at + 4] {
            b"fmt " => {
                if fmt.is_some() || n != 16 {
                    return Err("WAV duplicate/extended fmt unsupported".into());
                }
                let f = &b[start..end];
                let channels = u32::from(u16le(f, 2));
                let rate = u32le(f, 4);
                if u16le(f, 0) != 1
                    || !matches!(channels, 1 | 2)
                    || !(8000..=96000).contains(&rate)
                    || u16le(f, 14) != 16
                    || u32::from(u16le(f, 12)) != channels * 2
                    || u32le(f, 8) != rate.checked_mul(channels * 2).ok_or("WAV rate overflow")?
                {
                    return Err("WAV supports only PCM16LE mono/stereo 8000..96000 Hz".into());
                }
                fmt = Some((channels, rate));
            }
            b"data" => {
                if data.is_some() {
                    return Err("WAV duplicate data".into());
                }
                data = Some((start, end));
            }
            // These carry alternative timelines/data/references; never interpret or ignore as ordinary metadata.
            b"LIST" | b"plst" | b"cue " | b"smpl" | b"wavl" | b"slnt" | b"fact" | b"RIFF"
            | b"RIFX" | b"RF64" => {
                return Err("WAV timeline/external/alternate profile unsupported".into());
            }
            _ => {} // finite opaque unknown metadata; never instructions or references
        }
        at = next;
    }
    let (channels, rate) = fmt.ok_or("WAV missing fmt")?;
    let (start, end) = data.ok_or("WAV missing data")?;
    let length = end - start;
    if length == 0 || length > PCM_CAP || length % (channels as usize * 2) != 0 {
        return Err("WAV empty/partial/over-budget PCM frames".into());
    }
    let frames = length / (channels as usize * 2);
    let duration = frames as f64 / f64::from(rate);
    if !duration.is_finite() || duration <= 0.0 {
        return Err("WAV duration refused".into());
    }
    // All encoded/decoded/work limits established BEFORE allocation. At most 2,097,152 conversions.
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(length / 2)
        .map_err(|_| "PCM allocation refused")?;
    for (i, pair) in b[start..end].as_chunks::<2>().0.iter().enumerate() {
        if i % 4096 == 0 && cancel.load(Ordering::Acquire) {
            return Err("audio cancelled".into());
        }
        samples.push(i16::from_le_bytes([pair[0], pair[1]]));
    }
    if cancel.load(Ordering::Acquire) {
        return Err("audio cancelled".into());
    }
    Ok(Pcm {
        samples,
        channels,
        rate,
        frames,
        duration,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn wav(channels: u16) -> Vec<u8> {
        let mut b = b"RIFF".to_vec();
        b.extend(44u32.to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(channels.to_le_bytes());
        b.extend(8000u32.to_le_bytes());
        b.extend((8000u32 * u32::from(channels) * 2).to_le_bytes());
        b.extend((channels * 2).to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend(8u32.to_le_bytes());
        for n in [0i16, -32768, 32767, -1234] {
            b.extend(n.to_le_bytes());
        }
        b
    }
    #[test]
    fn actual_source_samples_and_frames() {
        for ch in [1, 2] {
            let p = decode(&wav(ch), &AtomicBool::new(false)).unwrap();
            assert_eq!(p.samples, [0, -32768, 32767, -1234]);
            assert_eq!(p.channels, u32::from(ch));
            assert_eq!(p.frames, 4 / ch as usize);
            assert_eq!(p.duration, p.frames as f64 / 8000.0);
            assert_eq!(p.rate, 8000);
        }
    }
    #[test]
    fn malformed_and_unsupported_refused() {
        let b = wav(1);
        for end in 0..b.len() {
            assert!(decode(&b[..end], &AtomicBool::new(false)).is_err());
        }
        for (offset, value) in [
            (20, 3),
            (22, 3),
            (24, 0),
            (28, 0),
            (32, 1),
            (34, 8),
            (40, 7),
        ] {
            let mut bad = b.clone();
            bad[offset] = value;
            assert!(decode(&bad, &AtomicBool::new(false)).is_err());
        }
        let mut trailer = b.clone();
        trailer.push(0);
        assert!(decode(&trailer, &AtomicBool::new(false)).is_err());
        assert!(decode(&b, &AtomicBool::new(true)).is_err());
        let mut duplicate = b.clone();
        duplicate.extend(&b[12..36]);
        let size = (duplicate.len() - 8) as u32;
        duplicate[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(decode(&duplicate, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn opaque_odd_metadata_and_iteration_budget() {
        let mut b = wav(1);
        b.extend(b"JUNK");
        b.extend(1u32.to_le_bytes());
        b.extend([42, 0]);
        let size = (b.len() - 8) as u32;
        b[4..8].copy_from_slice(&size.to_le_bytes());
        assert_eq!(
            decode(&b, &AtomicBool::new(false)).unwrap().samples[3],
            -1234
        );
        for _ in 0..CHUNK_CAP {
            b.extend(b"JUNK\0\0\0\0");
        }
        let size = (b.len() - 8) as u32;
        b[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(decode(&b, &AtomicBool::new(false)).is_err());
    }
}
