//! Generated SQLite databases and PNGs; no application artwork or private metadata.
use icatalog_media::{clip::extract_preview, prepare_media};
use rusqlite::{Connection, DatabaseName};

fn png() -> Vec<u8> {
    let mut state = 17u32;
    let image = image::RgbaImage::from_fn(128, 64, |x, _| {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        image::Rgba([
            (state >> 24) as u8,
            (state >> 16) as u8,
            state as u8,
            if x < 64 { 0 } else { 255 },
        ])
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

fn database(page_size: u32, preview: &[u8], extra: &str) -> Vec<u8> {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(&format!("PRAGMA page_size={page_size};
        CREATE TABLE Canvas (_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER, CanvasUnit INTEGER, CanvasWidth REAL, CanvasHeight REAL);
        INSERT INTO Canvas VALUES (1, 42, 0, 800, 400);
        CREATE TABLE CanvasPreview (_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER, CanvasId INTEGER, ImageType INTEGER, ImageWidth INTEGER, ImageHeight INTEGER, ImageData BLOB);
        {extra}")).unwrap();
    db.execute(
        "INSERT INTO CanvasPreview VALUES (1, 1, 42, 1, 128, 64, ?1)",
        [preview],
    )
    .unwrap();
    db.serialize(DatabaseName::Main).unwrap().to_vec()
}

fn chunk(out: &mut Vec<u8>, tag: &[u8; 8], body: &[u8]) {
    out.extend(tag);
    out.extend((body.len() as u64).to_be_bytes());
    out.extend(body);
}

fn clip(db: &[u8]) -> Vec<u8> {
    let mut out = b"CSFCHUNK".to_vec();
    out.extend(0u64.to_be_bytes());
    out.extend(24u64.to_be_bytes());
    let mut head = 256u64.to_be_bytes().to_vec();
    // An opaque layer contains a tempting unrelated PNG signature: no signature scanning.
    let layer = b"unrelated \x89PNG\r\n\x1a\n layer data";
    head.extend((80 + 16 + layer.len() as u64).to_be_bytes());
    head.extend(16u64.to_be_bytes());
    head.extend([0; 16]);
    chunk(&mut out, b"CHNKHead", &head);
    chunk(&mut out, b"CHNKExta", layer);
    chunk(&mut out, b"CHNKSQLi", db);
    chunk(&mut out, b"CHNKFoot", &[]);
    let len = out.len() as u64;
    out[8..16].copy_from_slice(&len.to_be_bytes());
    out
}

#[test]
fn saved_png_survives_overflow_pages_and_interior_schema_pages() {
    let png = png();
    let extra = (0..40)
        .map(|i| format!("CREATE TABLE Unrelated{i} (field INTEGER);"))
        .collect::<String>();
    for size in [512, 4096, 65536] {
        let bytes = clip(&database(size, &png, &extra));
        let preview = extract_preview(&bytes).unwrap();
        assert_eq!(preview.png, png);
        assert_eq!((preview.canvas_width, preview.canvas_height), (800, 400));
        assert_eq!((preview.preview_width, preview.preview_height), (128, 64));
        let prepared = prepare_media(&bytes, "CLIP", 1920).unwrap();
        assert_eq!((prepared.width, prepared.height), (Some(800), Some(400)));
        assert_eq!(
            (prepared.thumbnail.width, prepared.thumbnail.height),
            (128, 64)
        );
        assert_eq!(prepared.source, "clip_canvas_preview");
        let decoded = image::load_from_memory(&prepared.thumbnail.bytes)
            .unwrap()
            .to_rgb8();
        assert!(decoded
            .get_pixel(10, 10)
            .0
            .iter()
            .all(|channel| *channel > 250));
    }
}

#[test]
fn missing_ambiguous_or_invalid_canvas_previews_are_errors() {
    let png = png();
    let base = database(4096, &png, "");
    // Use actual SQLite to create cases instead of assuming record storage encodings.
    for (canvas, image_type, width, image_data, duplicate) in [
        (99, 1, 128, png.as_slice(), false),
        (42, 2, 128, png.as_slice(), false),
        (42, 1, 127, png.as_slice(), false),
        (42, 1, 128, b"not a PNG".as_slice(), false),
        (42, 1, 128, png.as_slice(), true),
    ] {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE Canvas (MainId INTEGER, CanvasUnit INTEGER, CanvasWidth REAL, CanvasHeight REAL);
            INSERT INTO Canvas VALUES (42, 0, 800, 400);
            CREATE TABLE CanvasPreview (CanvasId INTEGER, ImageType INTEGER, ImageWidth INTEGER, ImageHeight INTEGER, ImageData BLOB);").unwrap();
        db.execute(
            "INSERT INTO CanvasPreview VALUES (?1, ?2, ?3, 64, ?4)",
            rusqlite::params![canvas, image_type, width, image_data],
        )
        .unwrap();
        if duplicate {
            db.execute_batch("INSERT INTO Canvas SELECT * FROM Canvas")
                .unwrap();
        }
        let bytes = clip(&db.serialize(DatabaseName::Main).unwrap());
        assert!(prepare_media(&bytes, "clip", 256).is_err());
    }
    let no_preview = database(4096, &png, "DROP TABLE Canvas; CREATE TABLE Canvas (MainId INTEGER, CanvasUnit INTEGER, CanvasWidth REAL, CanvasHeight REAL);");
    assert!(extract_preview(&clip(&no_preview)).is_err());
    let mut bad_db = base;
    bad_db[56..60].copy_from_slice(&2u32.to_be_bytes()); // UTF-16 is not guessed.
    assert!(extract_preview(&clip(&bad_db)).is_err());
}

#[test]
fn chunk_lengths_and_truncations_are_checked() {
    let source = clip(&database(512, &png(), ""));
    for end in (0..source.len()).step_by(31) {
        let mut bytes = source[..end].to_vec();
        if bytes.len() >= 16 {
            bytes[8..16].copy_from_slice(&(end as u64).to_be_bytes());
        }
        assert!(extract_preview(&bytes).is_err(), "truncation {end}");
    }
    for offset in [8, 16, 32, 40, 48, 56, 88] {
        let mut bytes = source.clone();
        bytes[offset..offset + 8].copy_from_slice(&u64::MAX.to_be_bytes());
        assert!(extract_preview(&bytes).is_err(), "offset {offset}");
    }
    let mut bytes = source;
    bytes.extend(b"trailing bytes");
    let len = bytes.len() as u64;
    bytes[8..16].copy_from_slice(&len.to_be_bytes());
    assert!(extract_preview(&bytes).is_err());
}

#[test]
fn cyclic_overflow_and_table_pages_and_huge_payloads_are_errors() {
    let original = database(512, &png(), "");
    // Locate the preview's table leaf by its long payload; all other cells are small.
    let (page_index, cell_at, prefix, local) = original
        .as_chunks::<512>()
        .0
        .iter()
        .enumerate()
        .find_map(|(index, page)| {
            let head = craft_sqlite::parse_btree_page_header(page, index == 0).ok()?;
            if head.kind != 13 {
                return None;
            }
            for cell in 0..head.cells {
                let at = craft_sqlite::btree_cell_offset(page, &head, cell).ok()?;
                let (len, p) = craft_sqlite::read_varint(&page[at..]).ok()?;
                if len < 10000 {
                    continue;
                }
                let (_, rowid) = craft_sqlite::read_varint(&page[at + p..]).ok()?;
                let local = craft_sqlite::local_payload_len(
                    512,
                    len,
                    craft_sqlite::PayloadKind::TableLeaf,
                )?;
                return Some((index, at, p + rowid, local as usize));
            }
            None
        })
        .unwrap();
    let offset = page_index * 512 + cell_at;
    let overflow_at = offset + prefix + local;
    let overflow_number =
        u32::from_be_bytes(original[overflow_at..overflow_at + 4].try_into().unwrap());
    let mut cycle = original.clone();
    let start = (overflow_number as usize - 1) * 512;
    cycle[start..start + 4].copy_from_slice(&overflow_number.to_be_bytes());
    assert!(extract_preview(&clip(&cycle)).is_err());
    let mut huge = original.clone();
    huge[offset..offset + 9].fill(255);
    assert!(extract_preview(&clip(&huge)).is_err());
    let mut tree_cycle = original;
    let at = page_index * 512;
    tree_cycle[at] = 5;
    tree_cycle[at + 3..at + 5].fill(0);
    tree_cycle[at + 8..at + 12].copy_from_slice(&(page_index as u32 + 1).to_be_bytes());
    assert!(extract_preview(&clip(&tree_cycle)).is_err());
}

#[test]
fn mutated_databases_never_panic() {
    let original = database(512, &png(), "");
    for index in (0..original.len()).step_by(97) {
        let mut bytes = original.clone();
        bytes[index] ^= 255;
        let _ = extract_preview(&clip(&bytes));
    }
}
