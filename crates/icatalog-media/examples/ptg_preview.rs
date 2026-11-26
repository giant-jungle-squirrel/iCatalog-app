//! Local corpus check: extract a PNG and optionally compare a user-supplied export.
//! Does not embed or publish either input. Output creation refuses existing files.
use icatalog_media::ptg::extract_preview;
use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

fn bounded_read(path: &Path) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(512 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 512 * 1024 * 1024 {
        return Err("input exceeds 512 MiB".into());
    }
    Ok(bytes)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let source = args
        .next()
        .ok_or("usage: ptg_preview input.ptg new-output.png [reference.png]")?;
    let output = args.next().ok_or("missing output PNG path")?;
    let reference = args.next();
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let bytes = bounded_read(Path::new(&source))?;
    let preview = extract_preview(&bytes)?;
    // Full decode checks PNG corruption as well as the extractor's structural checks.
    let decoded = icatalog_media::decode_rgba(&preview.png)?;
    println!(
        "Canvas {}×{}; saved PNG {}×{} ({} bytes)",
        preview.canvas_width,
        preview.canvas_height,
        decoded.width,
        decoded.height,
        preview.png.len()
    );
    if let Some(reference) = reference {
        let export = icatalog_media::decode_rgba(&bounded_read(Path::new(&reference))?)?;
        if export.width != preview.canvas_width || export.height != preview.canvas_height {
            return Err("export dimensions do not match the source canvas".into());
        }
        let export = image::RgbaImage::from_raw(export.width, export.height, export.rgba)
            .ok_or("invalid export pixel buffer")?;
        let export = image::imageops::resize(
            &export,
            decoded.width,
            decoded.height,
            image::imageops::FilterType::Lanczos3,
        );
        let difference: u64 = decoded
            .rgba
            .iter()
            .zip(export.as_raw())
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        let mean = difference as f64 / decoded.rgba.len().max(1) as f64;
        println!("Mean absolute RGBA difference from resized export: {mean:.4} / 255");
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?
        .write_all(&preview.png)?;
    Ok(())
}
