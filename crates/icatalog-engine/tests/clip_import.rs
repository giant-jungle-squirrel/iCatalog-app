//! Generated SQLite/chunk fixtures only; no application artwork or private metadata.
use icatalog_engine::{dispatch, Context, Outcome, Params};
use std::{io::Cursor, path::Path};

fn chunk(bytes: &mut Vec<u8>, tag: &[u8; 8], payload: &[u8]) {
    bytes.extend(tag);
    bytes.extend((payload.len() as u64).to_be_bytes());
    bytes.extend(payload);
}

fn clip(root: &Path, value: u8) -> Vec<u8> {
    let mut png = Cursor::new(Vec::new());
    image::RgbImage::from_pixel(8, 4, image::Rgb([value, 60, 20]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let path = root.join(format!("fixture-{value}.sqlite"));
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE Canvas(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT,
          MainId INTEGER, CanvasUnit INTEGER, CanvasWidth REAL, CanvasHeight REAL);
         INSERT INTO Canvas(MainId, CanvasUnit, CanvasWidth, CanvasHeight)
          VALUES(1, 0, 80, 40);
         CREATE TABLE CanvasPreview(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT,
          MainId INTEGER, CanvasId INTEGER, ImageType INTEGER, ImageWidth INTEGER,
          ImageHeight INTEGER, ImageData BLOB);",
    )
    .unwrap();
    db.execute(
        "INSERT INTO CanvasPreview(MainId, CanvasId, ImageType, ImageWidth,
          ImageHeight, ImageData) VALUES(1, 1, 1, 8, 4, ?1)",
        [png.into_inner()],
    )
    .unwrap();
    db.close().unwrap();
    let database = std::fs::read(path).unwrap();
    let mut bytes = b"CSFCHUNK".to_vec();
    bytes.extend(0u64.to_be_bytes()); // Filled after the final chunk.
    bytes.extend(24u64.to_be_bytes());
    let mut head = Vec::new();
    head.extend(256u64.to_be_bytes());
    head.extend(80u64.to_be_bytes()); // CHNKSQLi follows the 40-byte head payload.
    head.extend(16u64.to_be_bytes());
    head.extend([0; 16]);
    chunk(&mut bytes, b"CHNKHead", &head);
    chunk(&mut bytes, b"CHNKSQLi", &database);
    chunk(&mut bytes, b"CHNKFoot", &[]);
    let length = bytes.len() as u64;
    bytes[8..16].copy_from_slice(&length.to_be_bytes());
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
fn clip_import_update_and_offline_preview_preserve_edits_and_last_good_data() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("icat-clip-{}-{nonce}", std::process::id()));
    let media = root.join("media");
    std::fs::create_dir_all(&media).unwrap();
    // Uppercase exercises extension routing independently of the PNG signature.
    let source = media.join("canvas.CLIP");
    std::fs::write(&source, clip(&root, 80)).unwrap();
    let catalogue = root.join("test.icatalog");
    let mut context = Context::open(&catalogue).unwrap();
    run(&mut context, "init", &[]);
    run(&mut context, "import", &[("path", media.to_str().unwrap())]);
    let original = context.catalog().asset(1).unwrap().unwrap();
    assert_eq!(original.format.as_deref(), Some("clip"));
    assert_eq!((original.width, original.height), (Some(80), Some(40)));
    let thumbnail = context.catalog().thumbnail(1, 256).unwrap().unwrap();
    assert_eq!(thumbnail.source, "clip_canvas_preview");
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

    // Failed decoding keeps the last successful source record and both renditions.
    std::fs::write(&source, b"CSFCHUNK damaged").unwrap();
    let Outcome::Table { rows, .. } = run(&mut context, "update", &[]) else {
        panic!("expected update summary")
    };
    assert!(rows.iter().any(|row| row[0]
        == "skipped: document preview unavailable (unsupported, damaged or exceeds limits)"
        && row[1] == "1"));
    run(&mut context, "previews", &[("action", "generate")]);
    assert_eq!(context.catalog().asset(1).unwrap().unwrap(), original);
    assert_eq!(
        context.catalog().thumbnail(1, 256).unwrap().unwrap(),
        thumbnail
    );
    assert_eq!(context.catalog().preview(1).unwrap().unwrap(), preview);
    assert_eq!(context.catalog().annotation(1).unwrap(), annotation);
    assert_eq!(context.catalog().asset_keywords(1).unwrap(), keywords);

    // A new saved canvas image updates the same asset without changing user edits.
    std::fs::write(&source, clip(&root, 200)).unwrap();
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
