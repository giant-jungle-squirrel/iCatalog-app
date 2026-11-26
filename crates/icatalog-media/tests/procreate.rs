//! Synthetic binary plists, ZIPs and original pixels; no application artwork.
use craft_plist::Value;
use craft_zip::{Compression, Writer};
use icatalog_media::{prepare_media, procreate::extract_preview};

fn string(value: &str) -> Value {
    Value::String(value.into())
}

fn dict(values: Vec<(&str, Value)>) -> Value {
    Value::Dict(values.into_iter().map(|(k, v)| (k.into(), v)).collect())
}

fn metadata(size: &str, version: i128, orientation: i128) -> Value {
    dict(vec![
        ("$archiver", string("NSKeyedArchiver")),
        ("$version", Value::Integer(100_000)),
        ("$top", dict(vec![("root", Value::Uid(1))])),
        (
            "$objects",
            Value::Array(vec![
                string("$null"),
                dict(vec![
                    ("$class", Value::Uid(2)),
                    ("size", Value::Uid(3)),
                    ("version", Value::Integer(version)),
                    ("orientation", Value::Integer(orientation)),
                    ("SilicaDocumentArchiveUnitKey", Value::Integer(0)),
                    // Intentional UID back-link is data, not binary plist recursion.
                    ("document", Value::Uid(1)),
                ]),
                dict(vec![("$classname", string("SilicaDocument"))]),
                string(size),
            ]),
        ),
    ])
}

// Tiny test-only binary plist writer: 4-byte references, 8-byte offsets.
fn plist(value: &Value) -> Vec<u8> {
    fn count(out: &mut Vec<u8>, kind: u8, len: usize) {
        if len < 15 {
            out.push(kind | len as u8);
        } else {
            out.extend([kind | 15, 0x12]);
            out.extend((len as u32).to_be_bytes());
        }
    }
    fn add(value: &Value, objects: &mut Vec<Vec<u8>>) -> u32 {
        let index = objects.len() as u32;
        objects.push(Vec::new());
        let mut out = Vec::new();
        match value {
            Value::String(s) => {
                assert!(s.is_ascii());
                count(&mut out, 0x50, s.len());
                out.extend(s.as_bytes());
            }
            Value::Integer(v) => {
                out.push(0x13);
                out.extend((*v as i64).to_be_bytes());
            }
            Value::Uid(v) => {
                out.push(0x87);
                out.extend(v.to_be_bytes());
            }
            Value::Array(values) => {
                count(&mut out, 0xa0, values.len());
                for value in values {
                    out.extend(add(value, objects).to_be_bytes());
                }
            }
            Value::Dict(values) => {
                count(&mut out, 0xd0, values.len());
                for (key, _) in values {
                    out.extend(add(&string(key), objects).to_be_bytes());
                }
                for (_, value) in values {
                    out.extend(add(value, objects).to_be_bytes());
                }
            }
            Value::Bool(v) => out.push(if *v { 9 } else { 8 }),
            _ => panic!("unsupported test value"),
        }
        objects[index as usize] = out;
        index
    }
    let mut objects = Vec::new();
    let top = add(value, &mut objects);
    let mut out = b"bplist00".to_vec();
    let mut offsets = Vec::new();
    for object in &objects {
        offsets.push(out.len() as u64);
        out.extend(object);
    }
    let table = out.len() as u64;
    for offset in offsets {
        out.extend(offset.to_be_bytes());
    }
    out.extend([0; 6]);
    out.extend([8, 4]);
    out.extend((objects.len() as u64).to_be_bytes());
    out.extend(u64::from(top).to_be_bytes());
    out.extend(table.to_be_bytes());
    out
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let pixels = image::RgbaImage::from_fn(width, height, |x, _| {
        image::Rgba([180, 40, 20, if x < width / 2 { 0 } else { 255 }])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    pixels.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn zip(entries: &[(&str, &[u8])], compression: Compression) -> Vec<u8> {
    let mut writer = Writer::new();
    for (name, data) in entries {
        writer.add(name, data, compression).unwrap();
    }
    writer.finish().unwrap()
}

fn document(meta: &Value, saved: &[u8]) -> Vec<u8> {
    zip(
        &[
            ("Document.archive", &plist(meta)),
            ("QuickLook/Thumbnail.png", saved),
        ],
        Compression::Stored,
    )
}

fn root_mut(meta: &mut Value) -> &mut Vec<(String, Value)> {
    let Value::Dict(fields) = meta else { panic!() };
    let objects = &mut fields.iter_mut().find(|(k, _)| k == "$objects").unwrap().1;
    let Value::Array(objects) = objects else {
        panic!()
    };
    let Value::Dict(root) = &mut objects[1] else {
        panic!()
    };
    root
}

#[test]
fn both_extensions_extract_same_saved_png_without_upscaling_or_rotating() {
    let saved = png(80, 40);
    for compression in [Compression::Stored, Compression::Deflated] {
        for (version, orientation, size) in [(1, 4, "{400, 800}"), (2, 1, "{800, 400}")] {
            let bytes = zip(
                &[
                    (
                        "Document.archive",
                        &plist(&metadata(size, version, orientation)),
                    ),
                    ("QuickLook/Thumbnail.png", &saved),
                    ("../layer.png", b"not followed or decoded"),
                    ("video/0.mp4", b"not decoded"),
                ],
                compression,
            );
            let preview = extract_preview(&bytes).unwrap();
            assert_eq!(preview.png, saved);
            assert_eq!((preview.canvas_width, preview.canvas_height), (800, 400));
            assert_eq!((preview.preview_width, preview.preview_height), (80, 40));
            for hint in ["PROCREATE", "ProCraft"] {
                assert!(icatalog_media::supported_extension(hint));
                let prepared = prepare_media(&bytes, hint, 1920).unwrap();
                assert_eq!(prepared.format, hint.to_ascii_lowercase());
                assert_eq!((prepared.width, prepared.height), (Some(800), Some(400)));
                assert_eq!(prepared.source, "procreate_quicklook");
                assert_eq!(
                    (prepared.thumbnail.width, prepared.thumbnail.height),
                    (80, 40)
                );
                assert_eq!(prepared.exif.orientation, None);
                let pixels = image::load_from_memory(&prepared.thumbnail.bytes)
                    .unwrap()
                    .to_rgb8();
                assert!(pixels.get_pixel(5, 5).0.iter().all(|v| *v > 250));
                assert!(pixels.get_pixel(70, 5).0[0] > 160);
            }
        }
    }
}

#[test]
fn keyed_archive_policy_rejects_ambiguous_invalid_and_unknown_metadata() {
    let good = metadata("{800, 400}", 2, 1);
    let saved = png(80, 40);
    for (key, bad) in [
        ("version", Value::Integer(3)),
        ("version", string("2")),
        ("orientation", Value::Integer(2)),
        ("orientation", Value::Integer(8)),
        ("orientation", Value::Bool(true)),
        ("SilicaDocumentArchiveUnitKey", Value::Integer(1)),
        ("size", Value::Uid(999_999)),
        ("size", Value::Uid(0)),
        ("size", Value::Uid(1)),
        ("size", Value::Integer(3)),
        ("$class", Value::Uid(u64::MAX)),
        ("$class", Value::Uid(3)),
    ] {
        let mut meta = good.clone();
        root_mut(&mut meta)
            .iter_mut()
            .find(|(k, _)| k == key)
            .unwrap()
            .1 = bad;
        assert!(extract_preview(&document(&meta, &saved)).is_err(), "{key}");
    }
    for key in [
        "version",
        "orientation",
        "SilicaDocumentArchiveUnitKey",
        "size",
        "$class",
    ] {
        let mut meta = good.clone();
        root_mut(&mut meta).retain(|(k, _)| k != key);
        assert!(extract_preview(&document(&meta, &saved)).is_err(), "{key}");
    }
    let mut duplicate = good.clone();
    root_mut(&mut duplicate).push(("size".into(), Value::Uid(3)));
    assert!(extract_preview(&document(&duplicate, &saved)).is_err());
    for bad in [
        "{0, 400}",
        "{-1, 400}",
        "{NaN, 400}",
        "{1.5, 400}",
        "{1000001, 400}",
        "{800,400,3}",
        "800,400",
        "{,400}",
        "{99999999999999999999,400}",
    ] {
        assert!(
            extract_preview(&document(&metadata(bad, 2, 1), &saved)).is_err(),
            "{bad}"
        );
    }
    let mut bad = good.clone();
    let Value::Dict(fields) = &mut bad else {
        panic!()
    };
    fields.iter_mut().find(|(k, _)| k == "$archiver").unwrap().1 = string("UnknownArchiver");
    assert!(extract_preview(&document(&bad, &saved)).is_err());
}

#[test]
fn requires_exact_saved_preview_and_fully_decodes_before_display() {
    let meta = plist(&metadata("{800, 400}", 2, 1));
    let saved = png(80, 40);
    for entries in [
        vec![
            ("Document.archive", meta.as_slice()),
            ("layer/Thumbnail.png", &saved),
        ],
        vec![("QuickLook/Thumbnail.png", saved.as_slice())],
        vec![
            ("Document.archive", b"bplist00invalid".as_slice()),
            ("QuickLook/Thumbnail.png", &saved),
        ],
        vec![
            ("Document.archive", meta.as_slice()),
            ("QuickLook/Thumbnail.png", b"not PNG"),
        ],
        vec![
            ("Document.archive", meta.as_slice()),
            ("QuickLook/Thumbnail.png", &saved[..45]),
        ],
    ] {
        assert!(prepare_media(&zip(&entries, Compression::Stored), "procreate", 256).is_err());
    }
    assert!(extract_preview(&document(&metadata("{800, 400}", 2, 1), &png(80, 80))).is_err());
    assert!(extract_preview(&document(&metadata("{8, 4}", 2, 1), &saved)).is_err());
}

#[test]
fn zip_and_plist_truncation_corruption_and_mutations_are_bounded() {
    let meta = plist(&metadata("{800, 400}", 2, 1));
    let saved = png(80, 40);
    let good = zip(
        &[
            ("Document.archive", &meta),
            ("QuickLook/Thumbnail.png", &saved),
        ],
        Compression::Stored,
    );
    for end in 0..good.len() {
        assert!(extract_preview(&good[..end]).is_err());
    }
    for end in 0..meta.len() {
        let bytes = zip(
            &[
                ("Document.archive", &meta[..end]),
                ("QuickLook/Thumbnail.png", &saved),
            ],
            Compression::Stored,
        );
        assert!(extract_preview(&bytes).is_err());
    }
    for at in (0..meta.len()).step_by(3) {
        let mut changed = meta.clone();
        changed[at] ^= 255;
        let bytes = zip(
            &[
                ("Document.archive", &changed),
                ("QuickLook/Thumbnail.png", &saved),
            ],
            Compression::Stored,
        );
        let _ = extract_preview(&bytes);
    }
    let at = good
        .windows(8)
        .position(|w| w == b"\x89PNG\r\n\x1a\n")
        .unwrap();
    let mut corrupted = good.clone();
    corrupted[at + 45] ^= 1;
    assert!(extract_preview(&corrupted).is_err());
    let mut duplicate = zip(
        &[
            ("Document.archive", &meta),
            ("QuickLook/Thumbnail.png", &saved),
            ("QuickLook/Thumbnaix.png", &saved),
        ],
        Compression::Stored,
    );
    let alternate = b"QuickLook/Thumbnaix.png";
    for at in 0..duplicate.len().saturating_sub(alternate.len()) {
        if duplicate.get(at..at + alternate.len()) == Some(alternate) {
            duplicate[at + 18] = b'l';
        }
    }
    assert!(extract_preview(&duplicate).is_err());
    let mut too_many = good.clone();
    let end = too_many.len() - 22;
    too_many[end + 8..end + 12].fill(254);
    assert!(extract_preview(&too_many).is_err());
}

#[test]
fn resource_budgets_reject_large_metadata_and_preview_geometry() {
    let saved = png(80, 40);
    let oversized = vec![0; 4 * 1024 * 1024 + 1];
    assert!(extract_preview(&zip(
        &[
            ("Document.archive", &oversized),
            ("QuickLook/Thumbnail.png", &saved)
        ],
        Compression::Deflated
    ))
    .is_err());
    let mut nested = metadata("{800, 400}", 2, 1);
    for _ in 0..40 {
        nested = Value::Array(vec![nested]);
    }
    assert!(extract_preview(&document(&nested, &saved)).is_err());

    let mut huge_png = png(2, 1);
    huge_png[16..20].copy_from_slice(&6000u32.to_be_bytes());
    huge_png[20..24].copy_from_slice(&4001u32.to_be_bytes());
    let mut crc = u32::MAX;
    for byte in &huge_png[12..29] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    huge_png[29..33].copy_from_slice(&(!crc).to_be_bytes());
    let error = extract_preview(&document(&metadata("{12000, 8002}", 2, 1), &huge_png))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("dimensions exceed preview or canvas limits"),
        "{error}"
    );
}
