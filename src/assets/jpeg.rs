//! Narrow baseline JPEG admission followed by the reviewed strict synchronous fork.
//! Full immutable-stream preflight and checked logical budget precede Decoder::new.
use super::Texture;
use crate::Result;
use jpeg_decoder::{CodingProcess, ColorTransform, Decoder, ImageInfo, PixelFormat};

const CAP: usize = 4 * 1024 * 1024;
const HEADER_CAP: usize = 256 * 1024;
const ANCILLARY_CAP: usize = 128 * 1024;
const MARKER_CAP: usize = 4096;
const LOGICAL_CAP: usize = 32 * 1024 * 1024;
// Table/header/COM/control payload allowance; derivation in JPEG-BACKEND.md.
// Not allocator capacity, RSS, native heap, GPU or application-wide accounting.
const TABLE_ALLOWANCE: usize = 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
struct Shape {
    width: u32,
    height: u32,
    rgba: usize,
}

fn shape(width: u32, height: u32) -> Result<Shape> {
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return Err("JPEG axes must be 1..1024".into());
    }
    let rgba = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= CAP)
        .ok_or("JPEG RGBA exceeds 4 MiB")?;
    Ok(Shape {
        width,
        height,
        rgba,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Sampling {
    h: usize,
    v: usize,
}

#[derive(Debug)]
struct Validated {
    shape: Shape,
    components: usize,
    sampling: [Sampling; 3],
}

fn preflight(bytes: &[u8]) -> Result<Validated> {
    if bytes.len() > CAP || !bytes.starts_with(&[0xff, 0xd8]) {
        return Err("JPEG signature/4 MiB encoded cap refused".into());
    }
    let mut at = 2;
    let mut markers = 1;
    let mut header = 2;
    let mut ancillary = 0;
    let mut dimensions = None;
    let mut components = 0_usize;
    let mut sampling = [Sampling::default(); 3];
    let mut quantizers = [0_u8; 3];
    let mut qt = 0_u8;
    let mut dc = 0_u8;
    let mut ac = 0_u8;
    let mut scanned = false;
    let mut entropy = false;
    let mut entropy_bytes = 0_usize;
    let mut restart_interval = 0_u16;
    let mut next_restart = 0_u8;
    let mut jfif = false;
    while at < bytes.len() {
        if entropy && bytes[at] != 0xff {
            at += 1;
            entropy_bytes += 1;
            continue;
        }
        if bytes[at] != 0xff {
            return Err("JPEG expected framed marker".into());
        }
        let start = at;
        at += 1;
        // Marker fill is legal, but FF FF 00 is not entropy byte stuffing.
        while bytes.get(at) == Some(&0xff) {
            at += 1;
        }
        let code = *bytes.get(at).ok_or("JPEG truncated marker")?;
        at += 1;
        if entropy && code == 0 && at == start + 2 {
            entropy_bytes += 1;
            continue;
        }
        markers += 1;
        if markers > MARKER_CAP {
            return Err("JPEG marker count exceeds 4096".into());
        }
        if entropy && (0xd0..=0xd7).contains(&code) {
            if restart_interval == 0 || code != 0xd0 + next_restart || entropy_bytes == 0 {
                return Err("JPEG restart framing/order refused".into());
            }
            next_restart = (next_restart + 1) % 8;
            continue;
        }
        if code == 0xd9 {
            if !entropy || entropy_bytes == 0 || at != bytes.len() {
                return Err("JPEG missing scan/entropy or trailing/multiframe data".into());
            }
            return Ok(Validated {
                shape: dimensions.ok_or("JPEG missing SOF0")?,
                components,
                sampling,
            });
        }
        if entropy {
            // Deliberately one all-component baseline scan. No tables/metadata
            // after entropy, multiple scans, DNL or hierarchical resumption.
            return Err("JPEG only one complete interleaved baseline scan supported".into());
        }
        let len_bytes = bytes
            .get(at..at + 2)
            .ok_or("JPEG truncated segment length")?;
        let length = u16::from_be_bytes([len_bytes[0], len_bytes[1]]) as usize;
        if length < 2 {
            return Err("JPEG invalid segment length".into());
        }
        let end = at
            .checked_add(length)
            .filter(|n| *n <= bytes.len())
            .ok_or("JPEG segment exceeds encoded bytes")?;
        let data = &bytes[at + 2..end];
        header += end - start;
        if header > HEADER_CAP {
            return Err("JPEG headers exceed 256 KiB".into());
        }
        match code {
            0xe0 => {
                ancillary += length;
                // JFIF only, no embedded thumbnail/JFXX or opaque APP payload.
                if jfif || data.len() != 14 || &data[..5] != b"JFIF\0"
                    || data[5] != 1 || data[6] > 2 || data[7] > 2
                    || data[8..10] == [0, 0] || data[10..12] == [0, 0]
                    || data[12..14] != [0, 0]
                { return Err("JPEG APP0 requires single ordinary JFIF without thumbnail".into()); }
                jfif = true;
            }
            0xfe => ancillary += length, // COM opaque; never authority or expanded text.
            0xe1..=0xef => return Err("JPEG APP metadata unsupported (EXIF/orientation/ICC/Adobe/custom/multiframe); raw retained".into()),
            0xc0 => {
                if dimensions.is_some() || data.len() < 6 || data[0] != 8 {
                    return Err("JPEG requires one 8-bit baseline SOF0".into());
                }
                components = data[5] as usize;
                if !matches!(components, 1 | 3) || data.len() != 6 + 3 * components {
                    return Err("JPEG supports gray or three conventional YCbCr components only".into());
                }
                dimensions = Some(shape(u16::from_be_bytes([data[3], data[4]]) as u32,
                    u16::from_be_bytes([data[1], data[2]]) as u32)?);
                let mut blocks = 0;
                for (i, component) in data[6..].as_chunks::<3>().0.iter().enumerate() {
                    let (h, v) = (component[1] >> 4, component[1] & 15);
                    if component[0] != (i + 1) as u8 || !(1..=4).contains(&h)
                        || !(1..=4).contains(&v) || component[2] > 3
                        || (components == 1 && component[1] != 0x11)
                    { return Err("JPEG component IDs/sampling/quantizer outside narrow profile".into()); }
                    blocks += h as usize * v as usize;
                    quantizers[i] = component[2];
                    // The SAME validated SOF0, not a second permissive parser.
                    sampling[i] = Sampling { h: h as usize, v: v as usize };
                }
                if blocks > 10 { return Err("JPEG MCU block budget refused".into()); }
            }
            0xdb => {
                if data.is_empty() { return Err("JPEG empty DQT".into()); }
                let mut table = 0;
                while table < data.len() {
                    let id = data[table];
                    let values = data.get(table + 1..table + 65).ok_or("JPEG truncated DQT")?;
                    if id > 3 || values.contains(&0) || qt & (1 << id) != 0 {
                        return Err("JPEG needs unique nonzero 8-bit quantization tables 0..3".into());
                    }
                    qt |= 1 << id;
                    table += 65;
                }
            }
            0xc4 => {
                if data.is_empty() { return Err("JPEG empty DHT".into()); }
                let mut table = 0;
                while table < data.len() {
                    let head = data.get(table..table + 17).ok_or("JPEG truncated DHT")?;
                    let (class, id) = (head[0] >> 4, head[0] & 15);
                    if class > 1 || id > 3 { return Err("JPEG Huffman table selector refused".into()); }
                    let mut available = 1_i32;
                    let mut count = 0_usize;
                    for n in &head[1..] {
                        available = available * 2 - i32::from(*n);
                        count += *n as usize;
                        if available < 0 { return Err("JPEG oversubscribed Huffman table".into()); }
                    }
                    if count == 0 || count > 256 || available == 0 {
                        return Err("JPEG Huffman symbol/code budget refused".into());
                    }
                    let symbols = data.get(table + 17..table + 17 + count)
                        .ok_or("JPEG truncated Huffman symbols")?;
                    if symbols.iter().any(|s| if class == 0 { *s > 11 }
                        else { (*s & 15) > 10 || ((*s & 15) == 0 && !matches!(*s, 0 | 0xf0)) }) {
                        return Err("JPEG non-baseline Huffman symbols".into());
                    }
                    let mask = if class == 0 { &mut dc } else { &mut ac };
                    if *mask & (1 << id) != 0 { return Err("JPEG duplicate Huffman table".into()); }
                    *mask |= 1 << id;
                    table += 17 + count;
                }
            }
            0xdd => {
                if data.len() != 2 { return Err("JPEG invalid DRI".into()); }
                restart_interval = u16::from_be_bytes([data[0], data[1]]);
            }
            0xda => {
                if scanned || dimensions.is_none() || data.len() != 4 + 2 * components
                    || data[0] as usize != components || data[data.len() - 3..] != [0, 63, 0]
                { return Err("JPEG requires one all-component sequential baseline SOS".into()); }
                for i in 0..components {
                    let selector = data[2 + 2 * i];
                    let (d, a) = (selector >> 4, selector & 15);
                    if data[1 + 2 * i] != (i + 1) as u8 || d > 3 || a > 3
                        || qt & (1 << quantizers[i]) == 0
                        || dc & (1 << d) == 0 || ac & (1 << a) == 0
                    { return Err("JPEG scan references missing/unsupported component or table".into()); }
                }
                scanned = true;
                entropy = true;
            }
            _ => return Err("JPEG marker/profile unsupported (not baseline SOF0, hierarchical/lossless/progressive or malformed)".into()),
        }
        if ancillary > ANCILLARY_CAP {
            return Err("JPEG ancillary exceeds 128 KiB".into());
        }
        at = end;
    }
    Err("JPEG missing complete EOI".into())
}

fn checked_add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| "JPEG logical budget addition overflow".into())
}
fn checked_mul(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b)
        .ok_or_else(|| "JPEG logical budget multiplication overflow".into())
}
fn checked_ceil(n: usize, d: usize) -> Result<usize> {
    let extra = d.checked_sub(1).ok_or("JPEG logical budget zero divisor")?;
    Ok(checked_add(n, extra)? / d)
}
fn logical_total(terms: [usize; 7], limit: usize) -> Result<usize> {
    let total = terms.into_iter().try_fold(0, checked_add)?;
    if total > limit {
        return Err("JPEG logical payload estimate exceeds budget".into());
    }
    Ok(total)
}

#[derive(Debug)]
struct Budget {
    samples: usize,
    stride: usize,
    total: usize,
}

fn budget(encoded: usize, frame: &Validated, limit: usize) -> Result<Budget> {
    let Shape {
        width,
        height,
        rgba,
    } = frame.shape;
    let (w, h, c) = (width as usize, height as usize, frame.components);
    let sampling = frame
        .sampling
        .get(..c)
        .ok_or("JPEG budget component count")?;
    if !matches!(c, 1 | 3) || sampling.iter().any(|s| s.h == 0 || s.v == 0) {
        return Err("JPEG budget missing validated sampling".into());
    }
    let hm = sampling.iter().map(|s| s.h).max().ok_or("JPEG budget HM")?;
    let vm = sampling.iter().map(|s| s.v).max().ok_or("JPEG budget VM")?;
    let mx = checked_ceil(w, checked_mul(8, hm)?)?;
    let my = checked_ceil(h, checked_mul(8, vm)?)?;
    let mut planes = 0;
    let mut rows = 0;
    let mut max_width = 0;
    for s in sampling {
        let bx = checked_mul(mx, s.h)?;
        let by = checked_mul(my, s.v)?;
        planes = checked_add(planes, checked_mul(checked_mul(bx, by)?, 64)?)?;
        rows = checked_add(
            rows,
            checked_mul(checked_mul(checked_mul(bx, s.v)?, 64)?, 2)?,
        )?;
        max_width = max_width.max(checked_ceil(checked_mul(w, s.h)?, hm)?);
    }
    let pixels = checked_mul(w, h)?;
    let samples = checked_mul(c, pixels)?;
    let actual_rgba = checked_mul(4, pixels)?;
    let stride = checked_mul(w, 4)?;
    if actual_rgba != rgba || checked_mul(stride, h)? != rgba {
        return Err("JPEG validated RGBA shape mismatch".into());
    }
    let lines = checked_mul(checked_mul(c, max_width)?, hm)?;
    let total = logical_total(
        [
            encoded,
            planes,
            checked_mul(2, rows)?,
            samples,
            actual_rgba,
            lines,
            TABLE_ALLOWANCE,
        ],
        limit,
    )?;
    Ok(Budget {
        samples,
        stride,
        total,
    })
}

fn check_info(info: Option<ImageInfo>, frame: &Validated) -> Result<ImageInfo> {
    let info = info.ok_or("JPEG missing decoder info")?;
    let expected = if frame.components == 1 {
        PixelFormat::L8
    } else {
        PixelFormat::RGB24
    };
    if u32::from(info.width) != frame.shape.width
        || u32::from(info.height) != frame.shape.height
        || info.coding_process != CodingProcess::DctSequential
        || info.pixel_format != expected
    {
        return Err("JPEG decoder dimensions/process/sample format mismatch".into());
    }
    Ok(info)
}

pub(super) fn decode(bytes: &[u8]) -> Result<Texture> {
    let frame = preflight(bytes)?;
    let budget = budget(bytes.len(), &frame, LOGICAL_CAP)?;
    // All budget terms have been checked before even constructing the decoder.
    // The setter below is a LATE output-size check, NOT a preallocation guard.
    if budget.total > LOGICAL_CAP {
        return Err("JPEG logical payload estimate exceeds budget".into());
    }
    let mut decoder = Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_max_decoding_buffer_size(budget.samples);
    decoder.set_color_transform(if frame.components == 1 {
        ColorTransform::Grayscale
    } else {
        ColorTransform::YCbCr
    });
    decoder
        .read_info()
        .map_err(|e| format!("JPEG strict header refused: {e}"))?;
    let before = check_info(decoder.info(), &frame)?;
    let samples = decoder
        .decode()
        .map_err(|e| format!("JPEG strict decode refused: {e}"))?;
    let after = check_info(decoder.info(), &frame)?;
    if before != after || samples.len() != budget.samples {
        return Err("JPEG decoder changed info or sample length".into());
    }
    // No decoder/table/plane retention during the fallible outer RGBA reserve.
    drop(decoder);
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(frame.shape.rgba)
        .map_err(|_| "JPEG caller RGBA allocation refused")?;
    if frame.components == 1 {
        for &gray in &samples {
            rgba.extend_from_slice(&[gray, gray, gray, 255]);
        }
    } else {
        for rgb in samples.as_chunks::<3>().0.iter() {
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    drop(samples);
    if rgba.len() != frame.shape.rgba
        || budget.stride.checked_mul(frame.shape.height as usize) != Some(rgba.len())
    {
        return Err("JPEG actual RGBA length/stride mismatch".into());
    }
    // Alpha255 is standard gray/RGB conversion, not proof of entropy completion.
    // The reviewed strict fork, not opacity, supplies the bit/marker contract.
    Ok(Texture {
        width: frame.shape.width,
        height: frame.shape.height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const JPEG: &[u8] = include_bytes!("../../tests/fixtures/synthetic-jpeg-quadrants.jpg");
    fn insert(code: u8, data: &[u8]) -> Vec<u8> {
        let mut out = JPEG[..2].to_vec();
        out.extend_from_slice(&[0xff, code]);
        out.extend_from_slice(&u16::try_from(data.len() + 2).unwrap().to_be_bytes());
        out.extend_from_slice(data);
        out.extend_from_slice(&JPEG[2..]);
        out
    }
    fn sof() -> usize {
        JPEG.windows(2).position(|b| b == [0xff, 0xc0]).unwrap()
    }
    #[test]
    fn actual_fixture_and_opaque_comment_pass_framing() {
        assert_eq!(preflight(JPEG).unwrap().shape, shape(96, 64).unwrap());
        assert_eq!(
            preflight(&insert(0xfe, b"same codestream COM"))
                .unwrap()
                .shape,
            shape(96, 64).unwrap()
        );
        assert_eq!(shape(1024, 1024).unwrap().rgba, CAP);
        assert!(shape(1025, 1).is_err() && shape(0, 1).is_err());
    }
    #[test]
    fn metadata_profiles_and_dimension_precision_refused_before_decoder() {
        for code in 0xe1..=0xef {
            assert!(preflight(&insert(code, b"metadata")).is_err());
            assert!(decode(&insert(code, b"metadata")).is_err());
        }
        for (offset, value) in [(1, 0xc2), (4, 12), (9, 4)] {
            let mut bad = JPEG.to_vec();
            bad[sof() + offset] = value;
            assert!(preflight(&bad).is_err());
            assert!(decode(&bad).is_err());
        }
        let mut bad = JPEG.to_vec();
        bad[sof() + 7..sof() + 9].copy_from_slice(&1025_u16.to_be_bytes());
        assert!(preflight(&bad).is_err());
    }
    #[test]
    fn truncation_trailing_multiframe_lengths_and_budgets_refused() {
        for end in 0..JPEG.len() {
            assert!(preflight(&JPEG[..end]).is_err(), "prefix {end}");
        }
        let mut bad = JPEG.to_vec();
        bad.extend_from_slice(JPEG);
        assert!(preflight(&bad).is_err());
        assert!(preflight(&insert(0xfe, &vec![0; 65533])).is_ok());
        let mut bad = JPEG.to_vec();
        for _ in 0..3 {
            let mut next = bad[..2].to_vec();
            next.extend_from_slice(&[0xff, 0xfe, 0xff, 0xff]);
            next.extend_from_slice(&vec![0; 65533]);
            next.extend_from_slice(&bad[2..]);
            bad = next;
        }
        assert!(preflight(&bad).unwrap_err().contains("ancillary"));
        let mut bad = JPEG.to_vec();
        bad.splice(2..2, (0..4096).flat_map(|_| [0xff, 0xfe, 0, 2]));
        assert!(preflight(&bad).unwrap_err().contains("count"));
        assert!(preflight(&vec![0; CAP + 1]).is_err());
        assert!(preflight(&[0xff, 0xd8, 0xff, 0xfe, 0, 1]).is_err());
    }
    #[test]
    fn entropy_stuffing_restart_and_end_framing_are_not_segment_lengths() {
        // Structural controls only, NOT claimed decodable entropy.
        let sos = JPEG.windows(2).position(|b| b == [0xff, 0xda]).unwrap();
        let length = u16::from_be_bytes([JPEG[sos + 2], JPEG[sos + 3]]) as usize;
        let prefix = &JPEG[..sos + 2 + length];
        let mut stuffed = prefix.to_vec();
        stuffed.extend_from_slice(&[1, 0xff, 0, 2, 0xff, 0xd9]);
        assert!(preflight(&stuffed).is_ok());
        let mut restart = prefix.to_vec();
        restart.extend_from_slice(&[1, 0xff, 0xd0, 2, 0xff, 0xd9]);
        assert!(preflight(&restart).is_err());
        let mut dri = restart[..2].to_vec();
        dri.extend_from_slice(&[0xff, 0xdd, 0, 4, 0, 1]);
        dri.extend_from_slice(&restart[2..]);
        assert!(preflight(&dri).is_ok());
        let n = dri.len();
        dri[n - 4] = 0xd1;
        assert!(preflight(&dri).is_err());
    }
    #[test]
    fn actual_fixture_and_comment_have_same_opaque_source_pixels() {
        let a = decode(JPEG).unwrap();
        let b = decode(&insert(0xfe, b"same codestream COM")).unwrap();
        assert_eq!([a.width, a.height], [96, 64]);
        assert_eq!(a.rgba, b.rgba);
        for (x, y, color) in [
            (12, 8, [255_u8, 0, 0]),
            (82, 8, [0, 255, 0]),
            (12, 52, [0, 0, 255]),
            (82, 52, [255, 255, 0]),
        ] {
            let at = (y * 96 + x) * 4;
            assert!(
                a.rgba[at..at + 3]
                    .iter()
                    .zip(color)
                    .all(|(a, b)| a.abs_diff(b) <= 48)
            );
            assert_eq!(a.rgba[at + 3], 255);
        }
        assert_eq!(a.rgba.len(), 96 * 64 * 4);
        assert!(a.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    // Same minimized eighteen-real-bit gray control/adverse as the frozen probe.
    fn gray(entropy: &[u8]) -> Vec<u8> {
        fn segment(out: &mut Vec<u8>, marker: u8, payload: &[u8]) {
            out.extend_from_slice(&[255, marker]);
            out.extend_from_slice(&u16::try_from(payload.len() + 2).unwrap().to_be_bytes());
            out.extend_from_slice(payload);
        }
        let mut out = vec![255, 216];
        let mut q = vec![0];
        q.extend_from_slice(&[1; 64]);
        segment(&mut out, 219, &q);
        segment(&mut out, 192, &[8, 0, 24, 0, 24, 1, 1, 0x11, 0]);
        let mut h = Vec::new();
        for id in [0, 0x10] {
            h.push(id);
            h.push(1);
            h.extend_from_slice(&[0; 15]);
            h.push(0);
        }
        segment(&mut out, 196, &h);
        segment(&mut out, 218, &[1, 1, 0, 0, 63, 0]);
        out.extend_from_slice(entropy);
        out.extend_from_slice(&[255, 217]);
        out
    }
    #[test]
    fn gray_rgba_control_and_early_eoi_no_texture() {
        let good = gray(&[0, 0, 0x3f]);
        let frame = preflight(&good).unwrap();
        assert_eq!(frame.components, 1);
        assert_eq!(frame.sampling[0], Sampling { h: 1, v: 1 });
        let image = decode(&good).unwrap();
        assert_eq!([image.width, image.height], [24, 24]);
        assert_eq!(image.rgba.len(), 24 * 24 * 4);
        assert!(
            image
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [128, 128, 128, 255])
        );
        let bad = gray(&[0]);
        assert!(
            preflight(&bad).is_ok(),
            "framing alone is not entropy completion"
        );
        let error = decode(&bad).unwrap_err();
        assert!(error.contains("JPEG strict decode refused"), "{error}");
    }
    #[test]
    fn checked_budget_exact_boundary_and_overflow() {
        let frame = preflight(&gray(&[0, 0, 0x3f])).unwrap();
        let estimate = budget(123, &frame, LOGICAL_CAP).unwrap();
        // P=576; R=384; S=576; A=2304; L=24, plus E and T.
        assert_eq!(
            estimate.total,
            123 + 576 + 2 * 384 + 576 + 2304 + 24 + TABLE_ALLOWANCE
        );
        assert_eq!(estimate.samples, 576);
        assert_eq!(estimate.stride, 96);
        assert!(budget(123, &frame, estimate.total).is_ok());
        assert!(budget(123, &frame, estimate.total - 1).is_err());
        assert!(budget(usize::MAX, &frame, usize::MAX).is_err());
        assert_eq!(
            logical_total([LOGICAL_CAP, 0, 0, 0, 0, 0, 0], LOGICAL_CAP).unwrap(),
            LOGICAL_CAP
        );
        assert!(logical_total([LOGICAL_CAP, 1, 0, 0, 0, 0, 0], LOGICAL_CAP).is_err());
        assert!(logical_total([usize::MAX, 1, 0, 0, 0, 0, 0], usize::MAX).is_err());
        assert!(checked_mul(usize::MAX, 2).is_err());
        assert!(checked_ceil(usize::MAX, 8).is_err());
        assert!(checked_ceil(1, 0).is_err());
        assert_eq!(checked_ceil(9, 8).unwrap(), 2);
        // Ceil/padded odd 4:2:0 case: MX=MY=2, P=1536, R=1536, L=102.
        let odd = Validated {
            shape: shape(17, 19).unwrap(),
            components: 3,
            sampling: [
                Sampling { h: 2, v: 2 },
                Sampling { h: 1, v: 1 },
                Sampling { h: 1, v: 1 },
            ],
        };
        assert_eq!(
            budget(0, &odd, LOGICAL_CAP).unwrap().total,
            1536 + 2 * 1536 + 3 * 17 * 19 + 4 * 17 * 19 + 102 + TABLE_ALLOWANCE
        );
        for components in [1, 3] {
            let largest = Validated {
                shape: shape(1024, 1024).unwrap(),
                components,
                sampling: [Sampling { h: 1, v: 1 }; 3],
            };
            assert!(budget(CAP, &largest, LOGICAL_CAP).is_ok());
        }
    }
    #[test]
    fn signature_dispatch_and_png_only_alias_control() {
        assert_eq!(
            super::super::LocalImageFormat::detect(JPEG),
            Some(super::super::LocalImageFormat::Jpeg)
        );
        assert!(super::super::decode_local_png(JPEG).is_err());
        let direct = decode(JPEG).unwrap();
        let dispatched = super::super::decode_local_image(JPEG).unwrap();
        assert_eq!(
            [direct.width, direct.height],
            [dispatched.width, dispatched.height]
        );
        assert_eq!(direct.rgba, dispatched.rgba);
        // No filename is accepted by either API; suffix cannot change admission.
        let mut trailer = JPEG.to_vec();
        trailer.push(0);
        assert!(super::super::decode_local_image(&trailer).is_err());
    }
}
