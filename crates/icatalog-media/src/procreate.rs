//! Saved QuickLook PNGs in the observed Procreate/ProCraft ZIP subset.
//! No layers, video, keyed-archive objects or external paths are executed.
use crate::{MediaError, Result};
use craft_plist::Value;

const MAX_ARCHIVE: usize = 4 * 1024 * 1024;
const MAX_PNG: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: u64 = 10_000;

/// Original, already-oriented PNG and separate displayed canvas/preview geometry.
#[derive(Debug)]
pub struct ProcreatePreview {
    pub png: Vec<u8>,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub preview_width: u32,
    pub preview_height: u32,
}

fn error(message: impl std::fmt::Display) -> MediaError {
    MediaError::Decode(format!("Procreate preview: {message}"))
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| error(format!("missing {key}")))
}

// Interpret only the three needed UID links, never recursively instantiate or
// traverse the keyed archive graph. Real documents have intentional back-links.
fn object<'a>(objects: &'a [Value], value: &Value) -> Result<&'a Value> {
    let index = value
        .as_uid()
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| *v != 0)
        .ok_or_else(|| error("invalid keyed-archive reference"))?;
    objects
        .get(index)
        .ok_or_else(|| error("keyed-archive reference out of bounds"))
}

fn canvas_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    let archive = craft_plist::parse_binary_with_limits(
        bytes,
        craft_plist::Limits {
            max_input_bytes: MAX_ARCHIVE,
            max_objects: 100_000,
            max_nodes: 100_000,
            max_depth: 32,
            max_expanded_bytes: 8 * 1024 * 1024,
        },
    )
    .map_err(error)?;
    // The library preserves duplicate dictionary keys; reject ambiguity even in
    // currently ignored fields. This walk is bounded by the parser's node limit.
    let mut pending = vec![&archive];
    while let Some(value) = pending.pop() {
        match value {
            Value::Dict(entries) => {
                let mut names = std::collections::HashSet::new();
                for (key, value) in entries {
                    if !names.insert(key) {
                        return Err(error("duplicate plist dictionary key"));
                    }
                    pending.push(value);
                }
            }
            Value::Array(values) => pending.extend(values),
            _ => {}
        }
    }
    if field(&archive, "$archiver")?.as_str() != Some("NSKeyedArchiver")
        || field(&archive, "$version")?.as_int() != Some(100_000)
    {
        return Err(error("unsupported keyed archive"));
    }
    let objects = field(&archive, "$objects")?
        .as_array()
        .ok_or_else(|| error("invalid keyed-archive object array"))?;
    if objects.first().and_then(Value::as_str) != Some("$null") {
        return Err(error("invalid keyed-archive null object"));
    }
    let root = object(objects, field(field(&archive, "$top")?, "root")?)?;
    let class = object(objects, field(root, "$class")?)?;
    if field(class, "$classname")?.as_str() != Some("SilicaDocument")
        || !matches!(field(root, "version")?.as_int(), Some(1 | 2))
        || field(root, "SilicaDocumentArchiveUnitKey")?.as_int() != Some(0)
    {
        return Err(error("unsupported document class, version or canvas units"));
    }
    let size = object(objects, field(root, "size")?)?
        .as_str()
        .and_then(|s| s.strip_prefix('{'))
        .and_then(|s| s.strip_suffix('}'))
        .and_then(|s| s.split_once(','))
        .ok_or_else(|| error("invalid canvas size"))?;
    let dimension = |s: &str| -> Result<u32> {
        let s = s.trim();
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(error("canvas dimensions must be whole pixels"));
        }
        s.parse::<u32>()
            .ok()
            .filter(|v| (1..=1_000_000).contains(v))
            .ok_or_else(|| error("canvas dimensions exceed limits"))
    };
    let (width, height) = (dimension(size.0)?, dimension(size.1)?);
    match field(root, "orientation")?.as_int() {
        // Observed values only. QuickLook pixels already contain the display
        // orientation; this changes the canvas axes, never the saved PNG.
        Some(1) => Ok((width, height)),
        Some(4) => Ok((height, width)),
        _ => Err(error("unsupported document orientation")),
    }
}

/// Extract `QuickLook/Thumbnail.png` from an observed Procreate/ProCraft ZIP.
/// Validates ZIP checksums, bounded binary metadata and PNG header geometry.
/// Callers displaying pixels must fully decode the PNG; the adapter and tool do.
pub fn extract_preview(bytes: &[u8]) -> Result<ProcreatePreview> {
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
            max_total_bytes: MAX_PNG + MAX_ARCHIVE as u64,
        },
    )
    .map_err(error)?;
    if archive.has_duplicate_names() || archive.has_overlapping_files().map_err(error)? {
        return Err(error("duplicate or overlapping ZIP entries"));
    }
    let metadata = archive
        .read("Document.archive", MAX_ARCHIVE as u64)
        .map_err(error)?
        .ok_or_else(|| error("missing Document.archive"))?;
    let (canvas_width, canvas_height) = canvas_dimensions(&metadata)?;
    let png = archive
        .read("QuickLook/Thumbnail.png", MAX_PNG)
        .map_err(error)?
        .ok_or_else(|| error("no saved QuickLook/Thumbnail.png; layer rendering is unsupported"))?;
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(error("QuickLook/Thumbnail.png is not PNG data"));
    }
    let info = crate::probe(&png)?;
    let preview_width = u32::try_from(info.width).map_err(error)?;
    let preview_height = u32::try_from(info.height).map_err(error)?;
    if preview_width == 0
        || preview_height == 0
        || u64::from(preview_width) * u64::from(preview_height) > 24_000_000
        || preview_width > canvas_width
        || preview_height > canvas_height
    {
        return Err(error(
            "saved PNG dimensions exceed preview or canvas limits",
        ));
    }
    let aspect_error = (u64::from(preview_width) * u64::from(canvas_height))
        .abs_diff(u64::from(preview_height) * u64::from(canvas_width));
    if aspect_error > u64::from(canvas_width.max(canvas_height)) {
        return Err(error(
            "saved PNG aspect ratio disagrees with displayed canvas",
        ));
    }
    Ok(ProcreatePreview {
        png,
        canvas_width,
        canvas_height,
        preview_width,
        preview_height,
    })
}
