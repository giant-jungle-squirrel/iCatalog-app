//! Optional real-application corpus validation; never part of ordinary unit tests.
use icatalog_media::{content_hash, decode_rgba};
use serde::Deserialize;
use std::{
    error::Error,
    fs::File,
    io::Read,
    path::{Component, Path},
};

#[derive(Deserialize)]
struct Manifest {
    version: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    format: String,
    source: String,
    reference: String,
    source_sha256: String,
    reference_sha256: String,
    preview_sha256: String,
    canvas_width: u32,
    canvas_height: u32,
    preview_width: u32,
    preview_height: u32,
    max_mean_absolute_difference: f64,
}

fn read(path: &Path, limit: u64) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("corpus file exceeds size limit".into());
    }
    Ok(bytes)
}

fn input(root: &Path, name: &str, hash: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let path = Path::new(name);
    if path.as_os_str().is_empty()
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err("manifest paths must be relative paths within the corpus".into());
    }
    let path = root.join(path).canonicalize()?;
    if !path.starts_with(root) {
        return Err("sample symlink leaves corpus directory".into());
    }
    let bytes = read(&path, 512 * 1024 * 1024)?;
    if content_hash(&bytes) != format!("sha256:{hash}") {
        return Err(format!("SHA-256 mismatch: {name}").into());
    }
    Ok(bytes)
}

fn check(root: &Path, case: &Case) -> Result<(), Box<dyn Error>> {
    let tolerance = case.max_mean_absolute_difference;
    if !tolerance.is_finite() || !(0.0..=255.0).contains(&tolerance) {
        return Err("invalid comparison tolerance".into());
    }
    let source = input(root, &case.source, &case.source_sha256)?;
    let reference = input(root, &case.reference, &case.reference_sha256)?;
    let (png, source_size, saved_size) = match case.format.as_str() {
        "clip" => {
            let preview = icatalog_media::clip::extract_preview(&source)?;
            (
                preview.png,
                (preview.canvas_width, preview.canvas_height),
                (preview.preview_width, preview.preview_height),
            )
        }
        "reb" => {
            let preview = icatalog_media::reb::extract_preview(&source)?;
            (
                preview.png,
                (preview.canvas_width, preview.canvas_height),
                (preview.preview_width, preview.preview_height),
            )
        }
        "ptg" => {
            let preview = icatalog_media::ptg::extract_preview(&source)?;
            (
                preview.png,
                (preview.canvas_width, preview.canvas_height),
                (preview.preview_width, preview.preview_height),
            )
        }
        "procreate" | "procraft" => {
            let preview = icatalog_media::procreate::extract_preview(&source)?;
            (
                preview.png,
                (preview.canvas_width, preview.canvas_height),
                (preview.preview_width, preview.preview_height),
            )
        }
        other => return Err(format!("unsupported corpus format: {other}").into()),
    };
    if content_hash(&png) != format!("sha256:{}", case.preview_sha256) {
        return Err("extracted PNG hash differs".into());
    }
    let canvas = (case.canvas_width, case.canvas_height);
    let expected_preview = (case.preview_width, case.preview_height);
    if source_size != canvas || saved_size != expected_preview {
        return Err("extracted canvas/preview dimensions differ".into());
    }
    let pixels = decode_rgba(&png)?;
    let reference = decode_rgba(&reference)?;
    if (pixels.width, pixels.height) != expected_preview
        || (reference.width, reference.height) != canvas
    {
        return Err("decoded preview/export dimensions differ".into());
    }
    let reference = image::RgbaImage::from_raw(reference.width, reference.height, reference.rgba)
        .ok_or("invalid reference pixel buffer")?;
    let reference = image::imageops::resize(
        &reference,
        pixels.width,
        pixels.height,
        image::imageops::FilterType::Lanczos3,
    );
    let difference: u64 = pixels
        .rgba
        .iter()
        .zip(reference.as_raw())
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    let mean = difference as f64 / pixels.rgba.len().max(1) as f64;
    if mean > tolerance {
        return Err(format!("RGBA difference {mean:.4} exceeds {tolerance}").into());
    }
    println!(
        "{}: PASS (canvas {}×{}, preview {}×{}, RGBA difference {mean:.4}/255)",
        case.id, canvas.0, canvas.1, pixels.width, pixels.height
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or("usage: media_corpus manifest.toml")?;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let path = Path::new(&path).canonicalize().map_err(|err| {
        format!("corpus manifest unavailable; initialise the optional samples submodule: {err}")
    })?;
    let root = path.parent().ok_or("manifest has no directory")?;
    let manifest: Manifest = toml::from_str(std::str::from_utf8(&read(&path, 1024 * 1024)?)?)?;
    if manifest.version != 1 || manifest.cases.is_empty() || manifest.cases.len() > 1000 {
        return Err("unsupported manifest version or invalid case count".into());
    }
    let mut failed = 0;
    for case in &manifest.cases {
        if let Err(err) = check(root, case) {
            eprintln!("{}: FAIL: {err}", case.id);
            failed += 1;
        }
    }
    if failed > 0 {
        return Err(format!("{failed} corpus case(s) failed").into());
    }
    Ok(())
}
