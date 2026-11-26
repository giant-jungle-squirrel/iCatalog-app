//! Saved `.reb` canvas PNGs, based on the supplied format-8 sample.
//! See `docs/reb-format.md`. No layer rendering, external paths or file writes.
use crate::{MediaError, Result};

const MAX_XML: usize = 1024 * 1024;
const MAX_PNG: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: u64 = 10_000;

/// Original saved PNG bytes and separate document/preview geometry.
#[derive(Debug)]
pub struct RebPreview {
    pub png: Vec<u8>,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub preview_width: u32,
    pub preview_height: u32,
}

fn error(message: impl std::fmt::Display) -> MediaError {
    MediaError::Decode(format!("REB preview: {message}"))
}

fn dimension(canvas: &craft_xml::El, name: &str) -> Result<u32> {
    let value = canvas
        .attr(name)
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && (1.0..=1_000_000.0).contains(v) && v.fract() == 0.0)
        .ok_or_else(|| error("canvas dimensions must be positive whole pixels within limits"))?;
    Ok(value as u32)
}

fn canvas_dimensions(xml: &[u8]) -> Result<(u32, u32)> {
    let options = craft_xml::Options {
        doctype: craft_xml::Doctype::Reject,
        unknown_entity: craft_xml::UnknownEntity::Error,
        limits: craft_xml::Limits {
            max_input: MAX_XML,
            max_depth: 32,
            max_nodes: 20_000,
            max_elements: 10_000,
            max_attrs: 128,
            max_attrs_total: 40_000,
        },
        ..craft_xml::Options::default().strict()
    };
    let root = options
        .parse(xml)
        .map_err(|_| error("invalid or oversized artwork.xml"))?;
    // Reject ambiguity rather than trusting the first of repeated attributes.
    for element in std::iter::once(&root).chain(root.descendants()) {
        let mut names = std::collections::HashSet::new();
        for (name, _) in &element.attrs {
            if !names.insert(name) || name == "xmlns" || name.starts_with("xmlns:") {
                return Err(error("duplicate attributes or unsupported XML namespaces"));
            }
        }
    }
    if root.name != "aquarelle_artwork" || root.attr("file_format_version") != Some("8") {
        return Err(error(
            "unsupported artwork root or file format version (expected 8)",
        ));
    }
    let mut canvases = root.children("canvas");
    let canvas = canvases.next().ok_or_else(|| error("missing canvas"))?;
    if canvases.next().is_some() || canvas.attr("units") != Some("1") {
        return Err(error("ambiguous canvas or unsupported units"));
    }
    if canvas
        .attr("safe_boundary")
        .is_some_and(|value| value != "0")
    {
        return Err(error("unsupported canvas boundary"));
    }
    Ok((dimension(canvas, "width")?, dimension(canvas, "height")?))
}

/// Extract the one root `canvas.png` from an observed format-8 ZIP document.
/// Validates ZIP structure/checksums, XML and PNG header dimensions. Callers
/// displaying pixels must also fully decode the PNG (the media adapter does).
pub fn extract_preview(bytes: &[u8]) -> Result<RebPreview> {
    if bytes.len() > 512 * 1024 * 1024 {
        return Err(error("source exceeds 512 MiB limit"));
    }
    craft_zip::preflight::preflight(
        bytes,
        craft_zip::preflight::Limits {
            max_entries: MAX_ENTRIES,
            max_central_directory_bytes: 8 * 1024 * 1024,
            max_name_bytes: 1024,
        },
    )
    .map_err(error)?;
    let mut archive = craft_zip::Archive::open(
        bytes,
        craft_zip::Limits {
            max_entries: MAX_ENTRIES,
            max_total_bytes: MAX_PNG + MAX_XML as u64,
        },
    )
    .map_err(error)?;
    if archive.has_duplicate_names() || archive.has_overlapping_files().map_err(error)? {
        return Err(error("duplicate or overlapping ZIP entries"));
    }
    let xml = archive
        .read("artwork.xml", MAX_XML as u64)
        .map_err(error)?
        .ok_or_else(|| error("missing artwork.xml"))?;
    let (canvas_width, canvas_height) = canvas_dimensions(&xml)?;
    let png = archive
        .read("canvas.png", MAX_PNG)
        .map_err(error)?
        .ok_or_else(|| error("no saved canvas.png; layer rendering is unsupported"))?;
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(error("canvas.png is not PNG data"));
    }
    let info = crate::probe(&png)?;
    let preview_width = u32::try_from(info.width).map_err(error)?;
    let preview_height = u32::try_from(info.height).map_err(error)?;
    if u64::from(preview_width) * u64::from(preview_height) > 24_000_000
        || preview_width > canvas_width
        || preview_height > canvas_height
    {
        return Err(error(
            "saved PNG dimensions exceed preview or canvas limits",
        ));
    }
    // Allow one pixel of rounding in a reduced rendition, never a different canvas shape.
    let aspect_error = (u64::from(preview_width) * u64::from(canvas_height))
        .abs_diff(u64::from(preview_height) * u64::from(canvas_width));
    if aspect_error > u64::from(canvas_width.max(canvas_height)) {
        return Err(error("saved PNG aspect ratio disagrees with canvas"));
    }
    Ok(RebPreview {
        png,
        canvas_width,
        canvas_height,
        preview_width,
        preview_height,
    })
}
