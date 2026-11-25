//! Synthetic document integration: shared import/update/preview and failure preservation.
use icatalog_engine::{dispatch, Context, Outcome, Params};

fn psd(value: u8) -> Vec<u8> {
    let mut bytes = b"8BPS\0\x01".to_vec();
    bytes.extend([0; 6]);
    bytes.extend(3u16.to_be_bytes());
    bytes.extend(4u32.to_be_bytes());
    bytes.extend(8u32.to_be_bytes());
    bytes.extend(8u16.to_be_bytes());
    bytes.extend(3u16.to_be_bytes());
    bytes.extend([0; 14]); // colour/resources/layers lengths, raw compression
    bytes.extend([value; 96]);
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
fn creative_document_round_trip_preserves_edits_and_last_good_pixels() {
    let root = std::env::temp_dir().join(format!("icat-craft-roundtrip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let media = root.join("media");
    std::fs::create_dir_all(&media).unwrap();
    let source = media.join("canvas.psd");
    std::fs::write(&source, psd(80)).unwrap();
    let path = root.join("test.icatalog");
    let mut context = Context::open(&path).unwrap();
    run(&mut context, "init", &[]);
    run(&mut context, "import", &[("path", media.to_str().unwrap())]);
    let original = context.catalog().asset(1).unwrap().unwrap();
    assert_eq!(original.format.as_deref(), Some("psd"));
    assert_eq!((original.width, original.height), (Some(8), Some(4)));
    run(
        &mut context,
        "edit",
        &[
            ("id", "1"),
            ("rating", "5"),
            ("keyword-exact", "keep, exactly"),
        ],
    );
    run(&mut context, "previews", &[("action", "generate")]);
    let preview = context.catalog().preview(1).unwrap().unwrap();
    let thumbnail = context.catalog().thumbnail(1, 256).unwrap().unwrap();
    let annotation = context.catalog().annotation(1).unwrap();
    let keywords = context.catalog().asset_keywords(1).unwrap();
    // A damaged replacement must not overwrite the last successfully catalogued source.
    std::fs::write(&source, b"8BPS damaged").unwrap();
    let result = run(&mut context, "update", &[]);
    let Outcome::Table { rows, .. } = result else {
        panic!("expected update summary")
    };
    assert!(rows.iter().any(|row| row[0]
        == "skipped: document preview unavailable (unsupported, damaged or exceeds limits)"
        && row[1] == "1"));
    run(&mut context, "previews", &[("action", "generate")]);
    assert_eq!(context.catalog().asset(1).unwrap().unwrap(), original);
    assert_eq!(context.catalog().preview(1).unwrap().unwrap(), preview);
    assert_eq!(
        context.catalog().thumbnail(1, 256).unwrap().unwrap(),
        thumbnail
    );
    assert_eq!(context.catalog().annotation(1).unwrap(), annotation);
    assert_eq!(context.catalog().asset_keywords(1).unwrap(), keywords);
    // A later valid replacement updates pixels in place, keeping user annotations.
    let mut replacement = psd(200);
    replacement.push(0); // change size as well as content, independently of clock granularity
    std::fs::write(&source, replacement).unwrap();
    run(&mut context, "update", &[]);
    run(&mut context, "previews", &[("action", "generate")]);
    let changed = context.catalog().asset(1).unwrap().unwrap();
    assert_eq!(changed.uid, original.uid);
    assert_ne!(changed.content_hash, original.content_hash);
    assert_ne!(
        context.catalog().preview(1).unwrap().unwrap().hash,
        preview.hash
    );
    assert_eq!(context.catalog().annotation(1).unwrap(), annotation);
    assert_eq!(context.catalog().asset_keywords(1).unwrap(), keywords);
    drop(context);
    std::fs::remove_dir_all(media).unwrap();
    let mut reopened = Context::open(&path).unwrap();
    assert!(matches!(
        run(&mut reopened, "preview", &[("id", "1")]),
        Outcome::Preview(_)
    ));
}
