//! Generated chunk/PNG fixtures only; no application artwork or private metadata.
use icatalog_media::{prepare_media, ptg::extract_preview};

const HEADER: &[u8] = b"Ambient Design Project file. \r\nArtRage 2\r\n\0\0\x03\0";
const CANVAS: u32 = 0xff00_3002;
const PREVIEW: u32 = 0xff00_3b00;

fn chunk(tag: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = tag.to_le_bytes().to_vec();
    bytes.extend((payload.len() as u64).to_le_bytes());
    bytes.extend(payload);
    bytes
}

fn canvas(width: u32, height: u32) -> Vec<u8> {
    let mut bytes: Vec<u8> = "ArtRage Vitae\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    bytes
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let pixels = image::RgbaImage::from_fn(width, height, |x, _| {
        image::Rgba([160, 40, 20, if x < width / 2 { 0 } else { 255 }])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    pixels.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn document(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = HEADER.to_vec();
    for chunk in chunks {
        bytes.extend(chunk);
    }
    bytes
}

fn ptg(width: u32, height: u32, saved: &[u8]) -> Vec<u8> {
    document(&[chunk(CANVAS, &canvas(width, height)), chunk(PREVIEW, saved)])
}

#[test]
fn chooses_only_top_level_canvas_and_composites_transparency_on_white() {
    let saved = png(80, 40);
    let bytes = document(&[
        chunk(0xff00_3081, &png(8, 4)),
        chunk(0xff00_3086, &chunk(PREVIEW, &png(6, 3))),
        chunk(PREVIEW, &saved),
        chunk(CANVAS, &canvas(80, 40)),
        chunk(0xff00_9999, b"opaque unknown data"),
    ]);
    let preview = extract_preview(&bytes).unwrap();
    assert_eq!(preview.png, saved);
    assert_eq!((preview.canvas_width, preview.canvas_height), (80, 40));
    assert_eq!((preview.preview_width, preview.preview_height), (80, 40));
    let prepared = prepare_media(&bytes, "PTG", 1920).unwrap();
    assert_eq!((prepared.width, prepared.height), (Some(80), Some(40)));
    assert_eq!(
        (prepared.thumbnail.width, prepared.thumbnail.height),
        (80, 40)
    );
    assert_eq!(prepared.source, "ptg_canvas_preview");
    let pixels = image::load_from_memory(&prepared.thumbnail.bytes)
        .unwrap()
        .to_rgb8();
    assert!(pixels.get_pixel(5, 5).0.iter().all(|v| *v > 250));
    assert!(pixels.get_pixel(70, 5).0[0] > 140);
}

#[test]
fn missing_duplicate_or_unrecognised_canvas_is_not_replaced_with_another_image() {
    let saved = png(8, 4);
    let header = chunk(CANVAS, &canvas(8, 4));
    let preview = chunk(PREVIEW, &saved);
    for chunks in [
        vec![header.clone(), chunk(0xff00_3081, &saved)],
        vec![header.clone(), chunk(0xff00_3086, &preview)],
        vec![preview.clone()],
        vec![header.clone(), header.clone(), preview.clone()],
        vec![header.clone(), preview.clone(), preview.clone()],
        vec![header.clone(), chunk(PREVIEW, b"not PNG")],
    ] {
        assert!(extract_preview(&document(&chunks)).is_err());
    }
    let mut unsupported = canvas(8, 4);
    unsupported[0] = b'X';
    assert!(extract_preview(&document(&[chunk(CANVAS, &unsupported), preview.clone()])).is_err());
    let mut extended = canvas(8, 4);
    extended.extend([0; 4]);
    assert!(extract_preview(&document(&[chunk(CANVAS, &extended), preview])).is_err());
    let mut wrong_version = ptg(8, 4, &saved);
    wrong_version[44] = 4;
    assert!(extract_preview(&wrong_version).is_err());
    assert!(extract_preview(&saved).is_err());
}

#[test]
fn truncation_overflow_and_mutations_cannot_escape_as_panics() {
    let original = ptg(8, 4, &png(8, 4));
    for end in 0..original.len() {
        assert!(extract_preview(&original[..end]).is_err(), "prefix {end}");
    }
    for length in [u64::MAX, u64::MAX - 11, 512 * 1024 * 1024] {
        let mut bytes = original.clone();
        bytes[HEADER.len() + 4..HEADER.len() + 12].copy_from_slice(&length.to_le_bytes());
        assert!(extract_preview(&bytes).is_err());
    }
    let mut trailing = original.clone();
    trailing.push(0);
    assert!(extract_preview(&trailing).is_err());
    for at in 0..original.len() {
        let mut bytes = original.clone();
        bytes[at] ^= 255;
        let _ = extract_preview(&bytes);
        let _ = prepare_media(&bytes, "ptg", 256);
    }
}

#[test]
fn png_decode_corruption_and_mismatched_geometry_are_rejected() {
    let saved = png(8, 4);
    assert!(prepare_media(&ptg(8, 4, &saved[..45]), "ptg", 256).is_err());
    let mut bad = saved.clone();
    let idat = bad.windows(4).position(|b| b == b"IDAT").unwrap();
    bad[idat + 6] ^= 255;
    assert!(prepare_media(&ptg(8, 4, &bad), "ptg", 256).is_err());
    assert!(extract_preview(&ptg(16, 8, &saved)).is_err());
    for dimension in [0, 1_000_001, u32::MAX] {
        assert!(extract_preview(&ptg(dimension, 4, &saved)).is_err());
        assert!(extract_preview(&ptg(8, dimension, &saved)).is_err());
    }
    // Large declared dimensions must be rejected before allocating decoded pixels.
    let mut huge = saved;
    huge[16..20].copy_from_slice(&6000u32.to_be_bytes());
    huge[20..24].copy_from_slice(&4001u32.to_be_bytes());
    let mut crc = u32::MAX;
    for byte in &huge[12..29] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    huge[29..33].copy_from_slice(&(!crc).to_be_bytes());
    let error = extract_preview(&ptg(6000, 4001, &huge))
        .unwrap_err()
        .to_string();
    assert!(error.contains("exceed limits"), "{error}");
}

#[test]
fn chunk_and_saved_byte_budgets_bound_work() {
    let mut bytes = ptg(8, 4, &png(8, 4));
    for _ in 0..9998 {
        bytes.extend(chunk(0xff00_9999, &[]));
    }
    assert!(extract_preview(&bytes).is_ok());
    bytes.extend(chunk(0xff00_9999, &[]));
    assert!(extract_preview(&bytes)
        .unwrap_err()
        .to_string()
        .contains("too many"));

    // A complete oversized payload is rejected without copying or decoding it.
    let mut bytes = document(&[chunk(CANVAS, &canvas(8, 4))]);
    bytes.extend(PREVIEW.to_le_bytes());
    bytes.extend((64 * 1024 * 1024 + 1u64).to_le_bytes());
    bytes.resize(bytes.len() + 64 * 1024 * 1024 + 1, 0);
    assert!(extract_preview(&bytes)
        .unwrap_err()
        .to_string()
        .contains("64 MiB"));
}
