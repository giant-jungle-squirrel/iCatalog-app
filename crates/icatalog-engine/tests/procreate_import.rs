//! Generated ZIP/binary-plist/PNG fixtures only; no application artwork or private metadata.
use icatalog_engine::{dispatch, Context, Outcome, Params};
use std::io::Cursor;

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

// A minimal stored ZIP writer keeps media/container dependencies out of the engine.
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut directory = Vec::new();
    for (name, payload) in entries {
        let offset = bytes.len() as u32;
        let size = payload.len() as u32;
        let crc = crc32(payload);
        bytes.extend(b"PK\x03\x04");
        bytes.extend(20u16.to_le_bytes());
        bytes.extend([0; 8]); // Flags, stored compression, DOS time/date.
        bytes.extend(crc.to_le_bytes());
        bytes.extend(size.to_le_bytes());
        bytes.extend(size.to_le_bytes());
        bytes.extend((name.len() as u16).to_le_bytes());
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(name.as_bytes());
        bytes.extend(*payload);

        directory.extend(b"PK\x01\x02");
        directory.extend(20u16.to_le_bytes());
        directory.extend(20u16.to_le_bytes());
        directory.extend([0; 8]);
        directory.extend(crc.to_le_bytes());
        directory.extend(size.to_le_bytes());
        directory.extend(size.to_le_bytes());
        directory.extend((name.len() as u16).to_le_bytes());
        directory.extend([0; 12]); // Extra/comment lengths, disk, attributes.
        directory.extend(offset.to_le_bytes());
        directory.extend(name.as_bytes());
    }
    let offset = bytes.len() as u32;
    let size = directory.len() as u32;
    bytes.extend(directory);
    bytes.extend(b"PK\x05\x06");
    bytes.extend([0; 4]);
    bytes.extend((entries.len() as u16).to_le_bytes());
    bytes.extend((entries.len() as u16).to_le_bytes());
    bytes.extend(size.to_le_bytes());
    bytes.extend(offset.to_le_bytes());
    bytes.extend(0u16.to_le_bytes());
    bytes
}

// Authored synthetic NSKeyedArchiver binary plist, not bytes from an artwork.
// $top.root -> object 1: version 2, orientation 1, pixel unit 0,
// size -> object 3 "{80, 40}";
// $class -> object 2: SilicaDocument / ValkyrieDocument / NSObject.
const DOCUMENT: &[u8] = &[
    0x62, 0x70, 0x6c, 0x69, 0x73, 0x74, 0x30, 0x30, 0xd4, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x1b,
    0x1e, 0x59, 0x24, 0x61, 0x72, 0x63, 0x68, 0x69, 0x76, 0x65, 0x72, 0x58, 0x24, 0x6f, 0x62, 0x6a,
    0x65, 0x63, 0x74, 0x73, 0x54, 0x24, 0x74, 0x6f, 0x70, 0x58, 0x24, 0x76, 0x65, 0x72, 0x73, 0x69,
    0x6f, 0x6e, 0x5f, 0x10, 0x0f, 0x4e, 0x53, 0x4b, 0x65, 0x79, 0x65, 0x64, 0x41, 0x72, 0x63, 0x68,
    0x69, 0x76, 0x65, 0x72, 0xa4, 0x07, 0x08, 0x13, 0x1a, 0x55, 0x24, 0x6e, 0x75, 0x6c, 0x6c, 0xd5,
    0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x56, 0x24, 0x63, 0x6c, 0x61, 0x73,
    0x73, 0x5f, 0x10, 0x1c, 0x53, 0x69, 0x6c, 0x69, 0x63, 0x61, 0x44, 0x6f, 0x63, 0x75, 0x6d, 0x65,
    0x6e, 0x74, 0x41, 0x72, 0x63, 0x68, 0x69, 0x76, 0x65, 0x55, 0x6e, 0x69, 0x74, 0x4b, 0x65, 0x79,
    0x5b, 0x6f, 0x72, 0x69, 0x65, 0x6e, 0x74, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x54, 0x73, 0x69, 0x7a,
    0x65, 0x57, 0x76, 0x65, 0x72, 0x73, 0x69, 0x6f, 0x6e, 0x80, 0x02, 0x10, 0x00, 0x10, 0x01, 0x80,
    0x03, 0x10, 0x02, 0xd2, 0x14, 0x15, 0x16, 0x17, 0x58, 0x24, 0x63, 0x6c, 0x61, 0x73, 0x73, 0x65,
    0x73, 0x5a, 0x24, 0x63, 0x6c, 0x61, 0x73, 0x73, 0x6e, 0x61, 0x6d, 0x65, 0xa3, 0x17, 0x18, 0x19,
    0x5e, 0x53, 0x69, 0x6c, 0x69, 0x63, 0x61, 0x44, 0x6f, 0x63, 0x75, 0x6d, 0x65, 0x6e, 0x74, 0x5f,
    0x10, 0x10, 0x56, 0x61, 0x6c, 0x6b, 0x79, 0x72, 0x69, 0x65, 0x44, 0x6f, 0x63, 0x75, 0x6d, 0x65,
    0x6e, 0x74, 0x58, 0x4e, 0x53, 0x4f, 0x62, 0x6a, 0x65, 0x63, 0x74, 0x58, 0x7b, 0x38, 0x30, 0x2c,
    0x20, 0x34, 0x30, 0x7d, 0xd1, 0x1c, 0x1d, 0x54, 0x72, 0x6f, 0x6f, 0x74, 0x80, 0x01, 0x12, 0x00,
    0x01, 0x86, 0xa0, 0x00, 0x08, 0x00, 0x11, 0x00, 0x1b, 0x00, 0x24, 0x00, 0x29, 0x00, 0x32, 0x00,
    0x44, 0x00, 0x49, 0x00, 0x4f, 0x00, 0x5a, 0x00, 0x61, 0x00, 0x80, 0x00, 0x8c, 0x00, 0x91, 0x00,
    0x99, 0x00, 0x9b, 0x00, 0x9d, 0x00, 0x9f, 0x00, 0xa1, 0x00, 0xa3, 0x00, 0xa8, 0x00, 0xb1, 0x00,
    0xbc, 0x00, 0xc0, 0x00, 0xcf, 0x00, 0xe2, 0x00, 0xeb, 0x00, 0xf4, 0x00, 0xf7, 0x00, 0xfc, 0x00,
    0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x1f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    0x03,
];

fn procreate(value: u8, saved_canvas: bool) -> Vec<u8> {
    let mut png = Cursor::new(Vec::new());
    image::RgbImage::from_pixel(8, 4, image::Rgb([value, 60, 20]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    zip(&[
        ("Document.archive", DOCUMENT),
        // Layer pixels cannot substitute for the saved QuickLook rendition.
        (
            if saved_canvas {
                "QuickLook/Thumbnail.png"
            } else {
                "layer0.png"
            },
            png.get_ref(),
        ),
    ])
}

fn run(context: &mut Context, command: &str, pairs: &[(&str, &str)]) -> Outcome {
    let mut params = Params::new();
    for (key, value) in pairs {
        params.set(*key, *value);
    }
    dispatch(context, command, &params).unwrap()
}

#[test]
fn procreate_import_update_and_offline_preview_preserve_edits_and_last_good_data() {
    import_update_and_offline_preview("PROCREATE", "procreate");
}

#[test]
fn procraft_import_update_and_offline_preview_preserve_edits_and_last_good_data() {
    import_update_and_offline_preview("procraft", "procraft");
}

fn import_update_and_offline_preview(extension: &str, format: &str) {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("icat-{format}-{}-{nonce}", std::process::id()));
    let media = root.join("media");
    std::fs::create_dir_all(&media).unwrap();
    let source = media.join(format!("canvas.{extension}"));
    std::fs::write(&source, procreate(80, true)).unwrap();
    let catalogue = root.join("test.icatalog");
    let mut context = Context::open(&catalogue).unwrap();
    run(&mut context, "init", &[]);
    run(&mut context, "import", &[("path", media.to_str().unwrap())]);
    let original = context.catalog().asset(1).unwrap().unwrap();
    assert_eq!(original.format.as_deref(), Some(format));
    assert_eq!((original.width, original.height), (Some(80), Some(40)));
    let thumbnail = context.catalog().thumbnail(1, 256).unwrap().unwrap();
    assert_eq!(thumbnail.source, "procreate_quicklook");
    assert_eq!((thumbnail.width, thumbnail.height), (8, 4));
    run(
        &mut context,
        "edit",
        &[
            ("id", "1"),
            ("rating", "5"),
            ("keyword-exact", "keep, exactly"),
        ],
    );
    let annotation = context.catalog().annotation(1).unwrap();
    let keywords = context.catalog().asset_keywords(1).unwrap();
    run(&mut context, "previews", &[("action", "generate")]);
    let preview = context.catalog().preview(1).unwrap().unwrap();
    assert_eq!((preview.width, preview.height), (8, 4));

    // A malformed container and a valid archive without a saved canvas both keep
    // the last successful source record and both renditions, never showing a layer.
    for damaged in [b"PK damaged".to_vec(), procreate(200, false)] {
        std::fs::write(&source, damaged).unwrap();
        let Outcome::Table { rows, .. } = run(&mut context, "update", &[]) else {
            panic!("expected update summary")
        };
        assert!(rows.iter().any(|row| row[0]
            == "skipped: document preview unavailable (unsupported, damaged or exceeds limits)"
            && row[1] == "1"));
        assert_eq!(context.catalog().asset(1).unwrap().unwrap(), original);
        // Model an older asset without a content hash/generator marker: force
        // decoding instead of the current-preview or changed-source fast paths.
        let mut legacy = original.clone();
        legacy.content_hash = None;
        context.catalog().update_asset(&legacy).unwrap();
        // Changing the hash invalidates the reference by trigger. Seed the old
        // preview explicitly to construct the legacy state being tested.
        context.catalog().store_preview(&preview).unwrap();
        context
            .catalog()
            .delete_metadata_raw_for_asset_scheme(1, "preview_generator")
            .unwrap();
        let Outcome::Text(summary) = run(&mut context, "previews", &[("action", "generate")])
        else {
            panic!("expected preview generation summary")
        };
        assert!(
            summary.contains("generated 0; cached 0; unavailable, changed or unsupported 1"),
            "{summary}"
        );
        assert_eq!(context.catalog().asset(1).unwrap().unwrap(), legacy);
        assert_eq!(
            context.catalog().thumbnail(1, 256).unwrap().unwrap(),
            thumbnail
        );
        assert_eq!(context.catalog().preview(1).unwrap().unwrap(), preview);
        assert_eq!(context.catalog().annotation(1).unwrap(), annotation);
        assert_eq!(context.catalog().asset_keywords(1).unwrap(), keywords);
        context.catalog().update_asset(&original).unwrap();
        context.catalog().store_preview(&preview).unwrap();
    }

    std::fs::write(&source, procreate(200, true)).unwrap();
    run(&mut context, "update", &[]);
    run(&mut context, "previews", &[("action", "generate")]);
    let changed = context.catalog().asset(1).unwrap().unwrap();
    let changed_preview = context.catalog().preview(1).unwrap().unwrap();
    assert_eq!(changed.id, original.id);
    assert_eq!(changed.uid, original.uid);
    assert_eq!(changed.format.as_deref(), Some(format));
    assert_ne!(changed.content_hash, original.content_hash);
    assert_ne!(changed_preview.hash, preview.hash);
    assert_ne!(
        context.catalog().thumbnail(1, 256).unwrap().unwrap().bytes,
        thumbnail.bytes
    );
    assert_eq!(context.catalog().annotation(1).unwrap(), annotation);
    assert_eq!(context.catalog().asset_keywords(1).unwrap(), keywords);

    drop(context);
    std::fs::remove_dir_all(&media).unwrap();
    let mut reopened = Context::open_existing(&catalogue).unwrap();
    let Outcome::Preview(image) = run(&mut reopened, "preview", &[("id", "1")]) else {
        panic!("expected stored preview without the original file")
    };
    assert_eq!(image.hash, changed_preview.hash);
    assert_eq!((image.width, image.height), (8, 4));
    assert!(image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| p[0] > 180 && p[1] < 80 && p[2] < 40));
    drop(reopened);
    std::fs::remove_dir_all(root).unwrap();
}
