//! Shared, bounded source adapter for import, update and stored previews.
use crate::{ColorInfo, ExifData, MediaError, Result, Thumbnail};
use image::DynamicImage;
use std::io::Cursor;

const MAX_INPUT: usize = 512 * 1024 * 1024;
const MAX_ENTRY: usize = 64 * 1024 * 1024;
const MAX_PIXELS: u64 = 24_000_000;

/// Source metadata and one rendered rendition. Upstream types never leave this crate.
#[derive(Debug)]
pub struct PreparedMedia {
    pub format: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub exif: ExifData,
    pub metadata_errors: u64,
    pub color: ColorInfo,
    pub xmp_packet: Option<String>,
    pub thumbnail: Thumbnail,
    pub source: &'static str,
}

/// Extensions whose camera containers are read for saved previews before sensor decoding.
pub fn is_raw_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "dng"
            | "cr2"
            | "cr3"
            | "crw"
            | "nef"
            | "nrw"
            | "arw"
            | "sr2"
            | "srf"
            | "pef"
            | "raf"
            | "rwl"
            | "rw2"
            | "orf"
            | "srw"
            | "mrw"
            | "erf"
            | "mef"
            | "mos"
            | "3fr"
            | "fff"
            | "dcr"
            | "kdc"
            | "raw"
    )
}

/// Candidate extensions; acceptance still requires a usable, validated rendition.
pub fn supported_extension(ext: &str) -> bool {
    is_raw_extension(ext)
        || matches!(
            ext.to_ascii_lowercase().as_str(),
            "jpg"
                | "jpeg"
                | "png"
                | "gif"
                | "webp"
                | "bmp"
                | "tif"
                | "tiff"
                | "psd"
                | "psb"
                | "kra"
                | "ora"
                | "afphoto"
                | "afdesign"
                | "afpub"
                | "af"
                | "eps"
                | "pdf"
                | "ai"
                | "pcraft"
                | "vectorcraft"
                | "ecproj"
                | "clip"
                | "reb"
        )
}

pub fn prepare_media(bytes: &[u8], format_hint: &str, long_edge: u32) -> Result<PreparedMedia> {
    prepare_media_with_orientation(bytes, format_hint, long_edge, None)
}

/// A valid explicit orientation takes precedence over embedded metadata. This keeps
/// regeneration consistent with catalogue metadata, including older imports.
pub fn prepare_media_with_orientation(
    bytes: &[u8],
    format_hint: &str,
    long_edge: u32,
    orientation: Option<i64>,
) -> Result<PreparedMedia> {
    if bytes.len() > MAX_INPUT {
        return Err(error("source exceeds 512 MiB limit"));
    }
    if long_edge > 1920 {
        return Err(error("rendition exceeds 1920 pixel limit"));
    }
    let long_edge = if long_edge == 0 {
        crate::GRID_LONG_EDGE
    } else {
        long_edge
    };
    // Third-party decoders are fallible, but a defect must not escape into a catalogue transaction.
    std::panic::catch_unwind(|| prepare(bytes, format_hint, long_edge, orientation))
        .map_err(|_| error("media decoder failed unexpectedly"))?
}

fn error(message: impl ToString) -> MediaError {
    MediaError::Decode(message.to_string())
}

fn prepare(
    bytes: &[u8],
    hint: &str,
    long_edge: u32,
    override_orientation: Option<i64>,
) -> Result<PreparedMedia> {
    let hint = hint.to_ascii_lowercase();
    let mut exif = ExifData::default();
    let mut metadata_errors = 0;
    let mut color = ColorInfo::default();
    let mut packet = crate::xmp_packet(bytes);
    let mut source = "rendered";
    let (pixels, format, dimensions) = if is_raw_extension(&hint) {
        let result = prepare_raw(bytes, long_edge)?;
        exif = result.exif;
        metadata_errors = result.errors;
        packet = packet.or(result.packet);
        source = result.source;
        (result.pixels, hint, result.dimensions)
    } else if matches!(hint.as_str(), "psd" | "psb") {
        let result = prepare_psd(bytes)?;
        exif = result.exif;
        packet = result.packet.or(packet);
        color = result.color;
        source = result.source;
        (result.pixels, result.format, Some(result.dimensions))
    } else if hint == "clip" {
        let preview = crate::clip::extract_preview(bytes)?;
        source = "clip_canvas_preview";
        (
            crate::decode_image(&preview.png)?,
            hint,
            Some((preview.canvas_width, preview.canvas_height)),
        )
    } else if hint == "reb" {
        let preview = crate::reb::extract_preview(bytes)?;
        source = "reb_canvas_preview";
        (
            crate::decode_image(&preview.png)?,
            hint,
            Some((preview.canvas_width, preview.canvas_height)),
        )
    } else if hint == "kra" {
        let mut archive = craft_kra::Archive::open(bytes).map_err(error)?;
        let dims = (archive.image().width, archive.image().height);
        let pixels = archive_pixels(["mergedimage.png", "preview.png"], |name| {
            archive.entry(name, MAX_ENTRY).map_err(error)
        })?;
        (pixels, hint, Some(dims))
    } else if hint == "ora" {
        let mut archive = craft_ora::Archive::open(bytes).map_err(error)?;
        let dims = (archive.image().width, archive.image().height);
        let pixels = archive_pixels(["mergedimage.png", "Thumbnails/thumbnail.png"], |name| {
            archive.entry(name, MAX_ENTRY).map_err(error)
        })?;
        (pixels, hint, Some(dims))
    } else if matches!(hint.as_str(), "afphoto" | "afdesign" | "afpub" | "af") {
        let preview = craft_affinity::preview(bytes).map_err(error)?;
        // The thumbnail's dimensions are not the document canvas dimensions.
        (crate::decode_image(preview.png)?, hint, None)
    } else if hint == "eps" || (hint == "ai" && !bytes.starts_with(b"%PDF-")) {
        (prepare_eps(bytes)?, hint, None)
    } else if matches!(
        hint.as_str(),
        "pdf" | "ai" | "pcraft" | "vectorcraft" | "ecproj"
    ) {
        crate::craft_documents::prepare_document(bytes, &hint, long_edge)?
    } else {
        let info = crate::probe(bytes)?;
        if matches!(info.format.as_str(), "jpeg" | "png" | "tiff" | "webp") {
            match crate::read_exif(Cursor::new(bytes)) {
                Ok(value) => exif = value,
                Err(_) => metadata_errors += 1,
            }
        }
        color = crate::read_color_info(bytes).unwrap_or_default();
        (
            crate::decode_image(bytes)?,
            info.format,
            Some((info.width as u32, info.height as u32)),
        )
    };
    let dimensions = dimensions.filter(|(w, h)| *w != 0 && *h != 0);
    let orientation = override_orientation
        .filter(|v| (1..=8).contains(v))
        .or(exif.orientation);
    let pixels = if matches!(
        format.as_str(),
        "psd"
            | "clip"
            | "reb"
            | "psb"
            | "kra"
            | "ora"
            | "afphoto"
            | "afdesign"
            | "afpub"
            | "af"
            | "eps"
            | "ai"
            | "pdf"
            | "pcraft"
            | "vectorcraft"
            | "ecproj"
    ) {
        white_background(pixels)
    } else {
        pixels
    };
    let thumbnail = crate::encode_thumbnail(pixels, long_edge, orientation)?;
    Ok(PreparedMedia {
        format,
        width: dimensions.map(|d| i64::from(d.0)),
        height: dimensions.map(|d| i64::from(d.1)),
        exif,
        metadata_errors,
        color,
        xmp_packet: packet,
        thumbnail,
        source,
    })
}

fn archive_pixels<const N: usize>(
    names: [&str; N],
    mut entry: impl FnMut(&str) -> Result<Option<Vec<u8>>>,
) -> Result<DynamicImage> {
    for name in names {
        if let Ok(Some(bytes)) = entry(name) {
            if let Ok(image) = crate::decode_image(&bytes) {
                return Ok(image);
            }
        }
    }
    Err(MediaError::Unsupported(
        "document has no usable saved composite or thumbnail".into(),
    ))
}

fn tiff_options() -> craft_tiff::ParseOptions {
    craft_tiff::ParseOptions {
        max_ifds: 128,
        max_depth: 8,
        max_entries: 4096,
        max_entries_total: 16384,
        value_budget: Some(16 * 1024 * 1024),
        ..Default::default()
    }
}

fn craft_exif(bytes: &[u8]) -> Result<ExifData> {
    let m = craft_metadata::try_read_exif_with(bytes, &tiff_options()).map_err(error)?;
    Ok(ExifData {
        captured_local: m
            .capture_time
            .and_then(|d| crate::exif_datetime_to_iso(&d.to_exif())),
        tz_offset_min: m.capture_time.and_then(|d| d.offset_minutes).map(i64::from),
        make: m.make,
        model: m.model,
        lens: m.lens_model,
        focal_length_mm: m.focal_length.filter(|v| v.is_finite()),
        f_number: m.f_number.filter(|v| v.is_finite()),
        exposure_s: m.exposure_time.filter(|v| v.is_finite()),
        iso: m.iso.map(i64::from),
        gps_lat: m
            .gps
            .as_ref()
            .map(|g| g.latitude)
            .filter(|v| v.is_finite() && (-90.0..=90.0).contains(v)),
        gps_lon: m
            .gps
            .as_ref()
            .map(|g| g.longitude)
            .filter(|v| v.is_finite() && (-180.0..=180.0).contains(v)),
        orientation: m.orientation.map(|o| i64::from(o.to_exif())),
    })
}

struct RawPrepared {
    pixels: DynamicImage,
    dimensions: Option<(u32, u32)>,
    exif: ExifData,
    errors: u64,
    packet: Option<String>,
    source: &'static str,
}
fn prepare_raw(bytes: &[u8], long_edge: u32) -> Result<RawPrepared> {
    let mut limits = craft_raw::Limits::default();
    limits.max_samples = MAX_PIXELS as usize;
    limits.max_lossy_tile_samples = MAX_PIXELS as usize;
    // Upstream RAW readers use their own TIFF defaults. Require a complete bounded
    // walk before entering them, so repeated/oversized metadata values cannot amplify
    // a small hostile TIFF into the upstream input-proportional allocation budget.
    let cr3 = if bytes.get(4..8) == Some(b"ftyp") {
        craft_metadata::cr3::parse_cr3(bytes)
    } else {
        None
    };
    let cr3_bounded = cr3
        .as_ref()
        .is_none_or(|c| c.cmt.iter().flatten().all(|block| bounded_tiff(block)));
    let container_packet = cr3
        .as_ref()
        .and_then(|c| c.xmp)
        .filter(|p| p.len() <= 1024 * 1024)
        .map(|p| String::from_utf8_lossy(p).into_owned());
    let craft_allowed = if bytes.starts_with(b"II") || bytes.starts_with(b"MM") {
        bounded_tiff(bytes)
    } else {
        true
    };
    let craft_allowed = craft_allowed && cr3_bounded;
    let mut header_limits = craft_raw::Limits::default();
    header_limits.max_samples = 256_000_000;
    let info = craft_allowed
        .then(|| craft_raw::probe_info_with(bytes, &header_limits).ok())
        .flatten();
    let dimensions = crate::raw_dimensions(bytes).or_else(|| {
        info.as_ref()
            .and_then(|i| Some((u32::try_from(i.width).ok()?, u32::try_from(i.height).ok()?)))
    });
    let mut errors = 0;
    let mut exif = match crate::read_exif(Cursor::new(bytes)) {
        Ok(e) => e,
        Err(_) => {
            errors += 1;
            ExifData::default()
        }
    };
    if let Ok(supplement) = craft_exif(bytes) {
        merge_missing(&mut exif, supplement);
    }
    if cr3_bounded {
        if let Some(supplement) = cr3
            .as_ref()
            .and_then(craft_metadata::cr3::merged_exif)
            .and_then(|b| craft_exif(&b).ok())
        {
            merge_missing(&mut exif, supplement);
            errors = 0;
        }
    }
    // Preserve the legacy largest-candidate selection, then try upstream's additional containers.
    let legacy = crate::raw_preview(bytes).ok();
    for preview in legacy.into_iter().chain(
        std::iter::once_with(|| {
            craft_allowed
                .then(|| craft_raw::embedded_preview(bytes))
                .flatten()
        })
        .flatten(),
    ) {
        if let Ok(pixels) = crate::decode_image(&preview) {
            if let Ok(supplement) = crate::read_exif(Cursor::new(&preview)) {
                merge_missing(&mut exif, supplement);
            }
            if let Some(info) = &info {
                exif.make = exif.make.or_else(|| info.camera.make.clone());
                exif.model = exif.model.or_else(|| info.camera.model.clone());
                exif.orientation = exif
                    .orientation
                    .or(Some(i64::from(info.orientation.to_exif())));
            }
            let packet = container_packet.or_else(|| crate::xmp_packet(&preview));
            return Ok(RawPrepared {
                pixels,
                dimensions,
                exif,
                errors,
                packet,
                source: "raw_preview",
            });
        }
    }
    if !craft_allowed {
        return Err(error("RAW metadata exceeds parser limits or is incomplete"));
    }
    if let Some(info) = &info {
        exif.make = exif.make.or_else(|| info.camera.make.clone());
        exif.model = exif.model.or_else(|| info.camera.model.clone());
        exif.orientation = exif
            .orientation
            .or(Some(i64::from(info.orientation.to_exif())));
    }
    use craft_develop::sensor::{Method, RawDevelopExt};
    let raw = craft_raw::decode_reduced(bytes, long_edge.max(256) as usize, &limits)
        .map_err(error)?
        .map(Ok)
        .unwrap_or_else(|| craft_raw::decode_with(bytes, &limits))
        .map_err(error)?;
    let (dw, dh) = raw.info().developed_size();
    if dw
        .checked_mul(dh)
        .is_none_or(|n| n == 0 || n as u64 > MAX_PIXELS)
        || !raw.pixel_aspect.is_finite()
        || raw.pixel_aspect <= 0.0
    {
        return Err(error("RAW developed dimensions exceed preview budget"));
    }
    let transform = craft_develop::sensor::color::camera_transform(
        &raw,
        craft_develop::sensor::color::as_shot_white_xy(&raw),
    );
    let wb = transform.wb.map(|v| {
        if v.is_finite() {
            v.clamp(0.0, 64.0)
        } else {
            1.0
        }
    });
    let rgb = raw.develop_balanced(Method::Bilinear, wb).map_err(error)?;
    let matrix = craft_color_transform::SRGB
        .from_xyz()
        .mul(&craft_color_transform::REC2020.to_xyz())
        .mul(&transform.matrix);
    let exposure = if transform.baseline_exposure.is_finite() {
        transform.baseline_exposure.clamp(-8.0, 8.0).exp2()
    } else {
        1.0
    };
    let mut pixels = Vec::with_capacity(rgb.data.len().saturating_mul(3));
    for [r, g, b] in rgb.data {
        for v in matrix.apply([
            f64::from(r * wb[0]),
            f64::from(g * wb[1]),
            f64::from(b * wb[2]),
        ]) {
            pixels.push(srgb(v * exposure));
        }
    }
    let width = u32::try_from(rgb.width).map_err(error)?;
    let height = u32::try_from(rgb.height).map_err(error)?;
    let pixels = image::RgbImage::from_raw(width, height, pixels)
        .ok_or_else(|| error("RAW pixel length mismatch"))?;
    let dimensions = dimensions.or_else(|| {
        Some((
            u32::try_from(raw.width).ok()?,
            u32::try_from(raw.height).ok()?,
        ))
    });
    exif.orientation = exif
        .orientation
        .or(Some(i64::from(raw.orientation.to_exif())));
    Ok(RawPrepared {
        pixels: DynamicImage::ImageRgb8(pixels),
        dimensions,
        exif,
        errors,
        packet: container_packet,
        source: "rendered",
    })
}
fn srgb(v: f64) -> u8 {
    let v = if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }) * 255.0)
        .round() as u8
}

struct PsdPrepared {
    pixels: DynamicImage,
    dimensions: (u32, u32),
    format: String,
    exif: ExifData,
    color: ColorInfo,
    packet: Option<String>,
    source: &'static str,
}
fn prepare_psd(bytes: &[u8]) -> Result<PsdPrepared> {
    use craft_psd::{ColorMode, HeadView};
    let head = HeadView::from_bytes(bytes).map_err(error)?;
    let h = &head.header;
    let dimensions = (h.width, h.height);
    let mut color = ColorInfo {
        model: match h.color_mode {
            ColorMode::Rgb => Some("RGB"),
            ColorMode::Grayscale => Some("Gray"),
            ColorMode::Cmyk => Some("CMYK"),
            ColorMode::Lab => Some("Lab"),
            _ => None,
        }
        .map(str::to_owned),
        profile: None,
    };
    let mut exif = ExifData::default();
    let mut packet = None;
    let mut thumbnail = None;
    let mut real_merged = true;
    for resource in head.resources_limited(4096) {
        let Ok(resource) = resource else { break };
        if resource.data.len() > 16 * 1024 * 1024 {
            continue;
        }
        match resource.id {
            1033 | 1036 if thumbnail.is_none() => {
                if let Ok(t) = craft_psd::resources::Thumbnail::from_bytes(resource.data) {
                    if t.format == 1 {
                        thumbnail = crate::decode_image(t.data).ok().map(|image| {
                            if resource.id == 1033 {
                                // Adobe's PSD specification: Photoshop 4 JFIF samples
                                // in resource1033 are BGR (1036 uses RGB).
                                let mut rgb = image.into_rgb8();
                                for pixel in rgb.pixels_mut() {
                                    pixel.0.swap(0, 2);
                                }
                                DynamicImage::ImageRgb8(rgb)
                            } else {
                                image
                            }
                        });
                    }
                }
            }
            1039 => {
                if let Some((model, description)) = crate::color::icc_info(resource.data) {
                    color.model = model.or(color.model);
                    color.profile =
                        Some(description.unwrap_or_else(|| "Embedded ICC profile".into()));
                }
            }
            1057 => {
                if let Ok(info) = craft_psd::resources::VersionInfo::from_bytes(resource.data) {
                    real_merged &= info.has_real_merged_data;
                }
            }
            1058 | 1059 => {
                if let Ok(e) = craft_exif(resource.data) {
                    exif = e;
                }
            }
            1060 if resource.data.len() <= 1024 * 1024 => {
                packet = Some(String::from_utf8_lossy(resource.data).into_owned());
            }
            _ => {}
        }
    }
    let (pixels, source) = if let Some(pixels) = thumbnail {
        (pixels, "psd_resource")
    } else {
        if !real_merged {
            return Err(MediaError::Unsupported(
                "PSD contains a placeholder composite and no usable saved thumbnail".into(),
            ));
        }
        // Merged-only reader skips layer records entirely. Decode only the color planes,
        // avoiding spot channels and accidental allocation of every layer in a document.
        if head.color_mode_data.len() > 1024 * 1024 {
            return Err(error("PSD color data exceeds limit"));
        }
        let merged =
            craft_psd::merged::Merged::open_without_resources(bytes, MAX_PIXELS).map_err(error)?;
        let channels = match h.color_mode {
            ColorMode::Rgb => 3,
            ColorMode::Grayscale => 1,
            _ => {
                return Err(MediaError::Unsupported(
                    "PSD composite color mode (saved JPEG thumbnail may still be usable)".into(),
                ))
            }
        };
        if !matches!(h.depth, 8 | 16 | 32) || usize::from(h.channels) < channels {
            return Err(error("unsupported PSD composite depth/channels"));
        }
        let count = u64::from(h.width)
            .checked_mul(u64::from(h.height))
            .filter(|n| *n > 0 && *n <= MAX_PIXELS)
            .ok_or_else(|| error("PSD canvas exceeds limit"))? as usize;
        let stride = usize::from(h.depth / 8);
        let plane_bytes = count
            .checked_mul(stride)
            .ok_or_else(|| error("PSD plane overflow"))?;
        let stored_channels = if matches!(merged.compression, 2 | 3) {
            usize::from(h.channels)
        } else {
            channels
        };
        if plane_bytes
            .checked_mul(stored_channels)
            .is_none_or(|size| size > 96 * 1024 * 1024)
        {
            return Err(error("PSD composite exceeds 96 MiB decoded channel limit"));
        }
        let raw_planes;
        let zip_planes;
        let planes: Vec<&[u8]> = if matches!(merged.compression, 2 | 3) {
            // ZIP encodes all channels together. Skip the layer section by its
            // checked length and borrow the compressed stream instead of loading layers.
            let at = usize::try_from(head.layer_section_offset).map_err(error)?;
            let length_bytes = if h.version.is_psb() { 8 } else { 4 };
            let length = bytes
                .get(at..at.saturating_add(length_bytes))
                .ok_or_else(|| error("short PSD layer length"))?;
            let length = match length {
                [a, b, c, d] => u64::from(u32::from_be_bytes([*a, *b, *c, *d])),
                [a, b, c, d, e, f, g, h] => u64::from_be_bytes([*a, *b, *c, *d, *e, *f, *g, *h]),
                _ => return Err(error("invalid PSD layer length")),
            };
            let start = at
                .checked_add(length_bytes)
                .and_then(|n| n.checked_add(usize::try_from(length).ok()?))
                .and_then(|n| n.checked_add(2))
                .ok_or_else(|| error("PSD layer offset overflow"))?;
            let encoded = bytes
                .get(start..)
                .ok_or_else(|| error("PSD layer section outside file"))?;
            zip_planes = craft_psd::compression::decode_planes(
                craft_psd::Compression::from_u16(merged.compression),
                encoded,
                &craft_psd::ImageData::layout(h),
            )
            .map_err(error)?;
            zip_planes
                .chunks_exact(plane_bytes)
                .take(channels)
                .collect()
        } else {
            raw_planes = merged.read_planes(channels).map_err(error)?;
            raw_planes.iter().map(Vec::as_slice).collect()
        };
        let mut rgb = Vec::with_capacity(count * 3);
        for i in 0..count {
            for channel in 0..3 {
                let plane = planes
                    .get(if channels == 1 { 0 } else { channel })
                    .ok_or_else(|| error("missing PSD plane"))?;
                let offset = i
                    .checked_mul(stride)
                    .ok_or_else(|| error("PSD offset overflow"))?;
                let sample = plane
                    .get(offset..offset.saturating_add(stride))
                    .ok_or_else(|| error("short PSD plane"))?;
                let value = match sample {
                    [v] | [v, _] => *v,
                    [a, b, c, d] => srgb(f64::from(f32::from_be_bytes([*a, *b, *c, *d]))),
                    _ => return Err(error("invalid PSD sample")),
                };
                rgb.push(value);
            }
        }
        let pixels = image::RgbImage::from_raw(h.width, h.height, rgb)
            .ok_or_else(|| error("PSD pixel length mismatch"))?;
        (DynamicImage::ImageRgb8(pixels), "rendered")
    };
    Ok(PsdPrepared {
        pixels,
        dimensions,
        format: if h.version.is_psb() { "psb" } else { "psd" }.into(),
        exif,
        color,
        packet,
        source,
    })
}

fn prepare_eps(bytes: &[u8]) -> Result<DynamicImage> {
    if craft_eps::sniff(bytes).is_none() {
        return Err(error("unrecognised EPS/PostScript container"));
    }
    let sections = craft_eps::sections(bytes).map_err(error)?;
    if let Some(tiff) = sections.tiff {
        if tiff.len() <= MAX_ENTRY {
            if let Ok(image) = crate::decode_image(tiff) {
                return Ok(image);
            }
        }
    }
    if let Some(bitmap) = craft_eps::epsi(sections.postscript) {
        let image = image::GrayImage::from_raw(bitmap.width, bitmap.height, bitmap.gray)
            .ok_or_else(|| error("EPSI pixel length mismatch"))?;
        return Ok(DynamicImage::ImageLuma8(image));
    }
    Err(MediaError::Unsupported(
        "EPS/Illustrator has no usable TIFF or EPSI preview; PostScript execution is unsupported"
            .into(),
    ))
}

fn bounded_tiff(bytes: &[u8]) -> bool {
    craft_tiff::Tiff::parse_with_status(bytes, &tiff_options()).is_ok_and(|(_, complete)| complete)
}
fn merge_missing(primary: &mut ExifData, other: ExifData) {
    macro_rules! fill { ($($field:ident),* $(,)?) => { $(primary.$field = primary.$field.take().or(other.$field);)* }; }
    fill!(
        captured_local,
        tz_offset_min,
        make,
        model,
        lens,
        focal_length_mm,
        f_number,
        exposure_s,
        iso,
        gps_lat,
        gps_lon,
        orientation
    );
}

// Saved document renditions use straight alpha. Preserve the existing raster
// policy, but explicitly show new document previews on a white page background.
fn white_background(image: DynamicImage) -> DynamicImage {
    if !image.color().has_alpha() {
        return image;
    }
    let mut rgba = image.into_rgba8();
    for pixel in rgba.pixels_mut() {
        let [r, g, b, a] = &mut pixel.0;
        let alpha = u32::from(*a);
        for channel in [r, g, b] {
            *channel = ((u32::from(*channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        }
        *a = 255;
    }
    DynamicImage::ImageRgba8(rgba)
}
