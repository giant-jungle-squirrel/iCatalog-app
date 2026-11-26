//! Generated PTG chunk/PNG fixtures only; no application artwork or private metadata.
use icatalog_engine::{dispatch, Context, Outcome, Params};
use std::io::Cursor;

fn chunk(tag: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = tag.to_le_bytes().to_vec();
    bytes.extend((payload.len() as u64).to_le_bytes());
    bytes.extend(payload);
    bytes
}

fn ptg(value: u8, saved_canvas: bool) -> Vec<u8> {
    let mut png = Cursor::new(Vec::new());
    image::RgbImage::from_pixel(8, 4, image::Rgb([value, 60, 20]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let mut canvas: Vec<u8> = "ArtRage Vitae\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    canvas.extend(8u32.to_le_bytes());
    canvas.extend(4u32.to_le_bytes());
    let mut bytes = b"Ambient Design Project file. \r\nArtRage 2\r\n\0\0\x03\0".to_vec();
    bytes.extend(chunk(0xff00_3002, &canvas));
    // A small-thumbnail tag cannot substitute for a saved canvas.
    bytes.extend(chunk(
        if saved_canvas {
            0xff00_3b00
        } else {
            0xff00_3081
        },
        png.get_ref(),
    ));
    bytes
}

fn run(context: &mut Context, command: &str, pairs: &[(&str, &str)]) -> Outcome {
    let mut params = Params::new();
    for (key, value) in pairs {
        params.set(*key, *value);
    }
    dispatch(context, command, &params).unwrap()
}

#[test]
fn ptg_import_update_and_offline_preview_preserve_edits_and_last_good_data() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("icat-ptg-{}-{nonce}", std::process::id()));
    let media = root.join("media");
    std::fs::create_dir_all(&media).unwrap();
    let source = media.join("canvas.PTG");
    std::fs::write(&source, ptg(80, true)).unwrap();
    let catalogue = root.join("test.icatalog");
    let mut context = Context::open(&catalogue).unwrap();
    run(&mut context, "init", &[]);
    run(&mut context, "import", &[("path", media.to_str().unwrap())]);
    let original = context.catalog().asset(1).unwrap().unwrap();
    assert_eq!(original.format.as_deref(), Some("ptg"));
    assert_eq!((original.width, original.height), (Some(8), Some(4)));
    let thumbnail = context.catalog().thumbnail(1, 256).unwrap().unwrap();
    assert_eq!(thumbnail.source, "ptg_canvas_preview");
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

    // A malformed container and a valid container without a saved canvas both keep
    // the last successful source record and both renditions, never showing a layer.
    for damaged in [b"PTG damaged".to_vec(), ptg(200, false)] {
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

    std::fs::write(&source, ptg(200, true)).unwrap();
    run(&mut context, "update", &[]);
    run(&mut context, "previews", &[("action", "generate")]);
    let changed = context.catalog().asset(1).unwrap().unwrap();
    let changed_preview = context.catalog().preview(1).unwrap().unwrap();
    assert_eq!(changed.uid, original.uid);
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
