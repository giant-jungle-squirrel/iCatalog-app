//! Original generated pixels/XML/ZIP fixtures; no application assets.
use craft_zip::{Compression, Writer};
use icatalog_media::{prepare_media, reb::extract_preview};

const XML: &str = "<aquarelle_artwork file_format_version='8'><canvas width='800.000000' height='400.000000' units='1' safe_boundary='0'/></aquarelle_artwork>";

fn png(width: u32, height: u32) -> Vec<u8> {
    let pixels = image::RgbaImage::from_fn(width, height, |x, _| {
        image::Rgba([160, 40, 20, if x < width / 2 { 0 } else { 255 }])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    pixels.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn archive(entries: &[(&str, &[u8])], compression: Compression) -> Vec<u8> {
    let mut writer = Writer::new();
    for (name, data) in entries {
        writer.add(name, data, compression).unwrap();
    }
    writer.finish().unwrap()
}

fn reb(xml: &str, png: &[u8]) -> Vec<u8> {
    archive(
        &[("artwork.xml", xml.as_bytes()), ("canvas.png", png)],
        Compression::Deflated,
    )
}

#[test]
fn extracts_only_saved_canvas_and_keeps_document_dimensions() {
    let saved = png(80, 40);
    for compression in [Compression::Stored, Compression::Deflated] {
        let bytes = archive(
            &[
                ("layer0.png", b"not decoded"),
                ("../paper.png", b"not opened"),
                ("artwork.xml", XML.as_bytes()),
                ("canvas.png", &saved),
            ],
            compression,
        );
        let preview = extract_preview(&bytes).unwrap();
        assert_eq!(preview.png, saved);
        assert_eq!((preview.canvas_width, preview.canvas_height), (800, 400));
        assert_eq!((preview.preview_width, preview.preview_height), (80, 40));
        let prepared = prepare_media(&bytes, "REB", 1920).unwrap();
        assert_eq!((prepared.width, prepared.height), (Some(800), Some(400)));
        assert_eq!(
            (prepared.thumbnail.width, prepared.thumbnail.height),
            (80, 40)
        );
        assert_eq!(prepared.source, "reb_canvas_preview");
        let pixels = image::load_from_memory(&prepared.thumbnail.bytes)
            .unwrap()
            .to_rgb8();
        assert!(pixels.get_pixel(5, 5).0.iter().all(|v| *v > 250));
        assert!(pixels.get_pixel(70, 5).0[0] > 140);
    }
}

#[test]
fn missing_ambiguous_or_damaged_saved_images_are_not_replaced_by_layers() {
    let saved = png(80, 40);
    for entries in [
        vec![
            ("artwork.xml", XML.as_bytes()),
            ("layer0.png", saved.as_slice()),
        ],
        vec![("canvas.png", saved.as_slice())],
        vec![
            ("artwork.xml", XML.as_bytes()),
            ("nested/canvas.png", saved.as_slice()),
        ],
        vec![("artwork.xml", XML.as_bytes()), ("canvas.png", b"bad PNG")],
    ] {
        assert!(prepare_media(&archive(&entries, Compression::Stored), "reb", 256).is_err());
    }
    let mut duplicate = archive(
        &[
            ("artwork.xml", XML.as_bytes()),
            ("canvas.png", &saved),
            ("canvaX.png", &saved),
        ],
        Compression::Stored,
    );
    for at in 0..duplicate.len().saturating_sub(9) {
        if duplicate.get(at..at + 10) == Some(b"canvaX.png") {
            duplicate[at + 5] = b's';
        }
    }
    assert!(extract_preview(&duplicate).is_err());
    // A structurally valid header with truncated IDAT must still fail full decode.
    assert!(prepare_media(&reb(XML, &saved[..45]), "reb", 256).is_err());
    assert!(extract_preview(&reb(XML, &png(80, 80))).is_err());
}

#[test]
fn unsupported_or_hostile_xml_is_rejected_without_guessing_dimensions() {
    let saved = png(80, 40);
    let mut bad = vec![
        XML.replace("version='8'", "version='9'"),
        XML.replace("units='1'", "units='0'"),
        XML.replace("safe_boundary='0'", "safe_boundary='1'"),
        XML.replace("width='800.000000'", "width='800' width='900'"),
        XML.replace("<canvas", "<nested><canvas")
            .replace("</aquarelle", "</nested></aquarelle"),
        XML.replace(
            "<canvas",
            "<canvas width='800' height='400' units='1'/><canvas",
        ),
        format!("{XML}{XML}"),
        format!("<!DOCTYPE aquarelle_artwork SYSTEM 'file:///not-opened'>{XML}"),
        XML.replace("800.000000", "&unknown;"),
        XML.replace("<aquarelle_artwork", "<aquarelle_artwork xmlns='unknown'"),
        XML.replace("</aquarelle_artwork>", ""),
        format!(
            "<aquarelle_artwork file_format_version='8'>{}{}{}",
            "<x>".repeat(40),
            "</x>".repeat(40),
            "</aquarelle_artwork>"
        ),
    ];
    for value in ["NaN", "inf", "-1", "0", "0.5", "1000001", "1e99"] {
        bad.push(XML.replace("800.000000", value));
    }
    for xml in bad {
        assert!(extract_preview(&reb(&xml, &saved)).is_err());
    }
    assert!(extract_preview(&reb(&" ".repeat(1024 * 1024 + 1), &saved)).is_err());
}

#[test]
fn zip_checksums_truncation_lengths_and_mutations_fail_safely() {
    let saved = png(80, 40);
    let original = archive(
        &[("artwork.xml", XML.as_bytes()), ("canvas.png", &saved)],
        Compression::Stored,
    );
    for end in 0..original.len() {
        assert!(extract_preview(&original[..end]).is_err());
    }
    let png_at = original
        .windows(8)
        .position(|w| w == b"\x89PNG\r\n\x1a\n")
        .unwrap();
    let mut corrupt = original.clone();
    corrupt[png_at + 50] ^= 1;
    assert!(extract_preview(&corrupt).is_err());
    let central = original
        .windows(4)
        .position(|w| w == b"PK\x01\x02")
        .unwrap();
    for offset in [20, 24, 42] {
        let mut corrupt = original.clone();
        corrupt[central + offset..central + offset + 4].fill(255);
        assert!(extract_preview(&corrupt).is_err());
    }
    let mut huge_count = original.clone();
    let end = huge_count.len() - 22;
    huge_count[end + 8..end + 12].fill(254);
    assert!(extract_preview(&huge_count).is_err());
    for at in (0..original.len()).step_by(7) {
        let mut bytes = original.clone();
        bytes[at] ^= 255;
        let _ = extract_preview(&bytes);
    }
}

#[test]
fn preview_geometry_is_bounded_before_pixel_allocation() {
    // Keep tiny encoded pixels but declare large IHDR geometry with a valid CRC.
    // Extraction only probes the header; attempting a decode here would be wrong.
    let mut saved = png(2, 1);
    saved[16..20].copy_from_slice(&6000u32.to_be_bytes());
    saved[20..24].copy_from_slice(&4001u32.to_be_bytes());
    let mut crc = u32::MAX;
    for byte in &saved[12..29] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    saved[29..33].copy_from_slice(&(!crc).to_be_bytes());
    let xml = XML
        .replace("800.000000", "12000")
        .replace("400.000000", "8002");
    let err = extract_preview(&reb(&xml, &saved)).unwrap_err().to_string();
    assert!(
        err.contains("dimensions exceed preview or canvas limits"),
        "{err}"
    );
    let xml = XML.replace("800.000000", "40").replace("400.000000", "20");
    let err = extract_preview(&reb(&xml, &png(80, 40)))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("dimensions exceed preview or canvas limits"),
        "{err}"
    );
}
