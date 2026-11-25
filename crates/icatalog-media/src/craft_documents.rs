//! Saved Craft renditions and first-page PDF previews. No application document engine.
use crate::{MediaError, Result};
use image::DynamicImage;

type DocumentImage = (DynamicImage, String, Option<(u32, u32)>);
const MAX_INPUT: usize = 64 * 1024 * 1024;

fn error(message: impl ToString) -> MediaError {
    MediaError::Decode(message.to_string())
}

pub(crate) fn prepare_document(bytes: &[u8], hint: &str, long_edge: u32) -> Result<DocumentImage> {
    if bytes.len() > MAX_INPUT {
        return Err(error("document exceeds 64 MiB preview limit"));
    }
    if matches!(hint, "pdf" | "ai") {
        return Ok((pdf(bytes, long_edge)?, hint.to_owned(), None));
    }
    let mut limits = craft_doc::Limits::default();
    limits.max_json = 16 * 1024 * 1024;
    limits.max_entry = 32 * 1024 * 1024;
    limits.max_total = 64 * 1024 * 1024;
    limits.max_depth = 64;
    limits.max_items = 100_000;
    let info = craft_doc::info(bytes, &limits).map_err(error)?;
    if !info.supported {
        return Err(MediaError::Unsupported(
            "newer Craft document version".into(),
        ));
    }
    let dimensions = craft_doc::outline(bytes, &limits)
        .ok()
        .and_then(|outline| outline.canvas)
        .map(|canvas| (canvas.width, canvas.height));
    // Each call has its own bounded budget; a broken optional image must not hide
    // an independently usable rendition, such as VectorCraft's compatible PDF.
    for kind in [
        craft_doc::RenditionKind::Composite,
        craft_doc::RenditionKind::Thumbnail,
        craft_doc::RenditionKind::Pdf,
    ] {
        if let Ok(Some(rendition)) = craft_doc::rendition(bytes, kind, &limits) {
            let pixels = if kind == craft_doc::RenditionKind::Pdf {
                pdf(&rendition.bytes, long_edge)
            } else {
                saved_image(&rendition.bytes)
            };
            if let Ok(pixels) = pixels {
                return Ok((pixels, info.kind.extension().to_owned(), dimensions));
            }
        }
    }
    Err(MediaError::Unsupported(
        "Craft document has no usable saved image or PDF rendition".into(),
    ))
}

fn saved_image(bytes: &[u8]) -> Result<DynamicImage> {
    let info = crate::probe(bytes)?;
    if info
        .width
        .checked_mul(info.height)
        .is_none_or(|n| n > 24_000_000)
    {
        return Err(error("saved rendition exceeds pixel budget"));
    }
    crate::decode_image(bytes)
}

/// Bound retained font copies as well as individual streams before rendering.
/// Parent/Kids traversal deliberately also counts resources on related pages.
fn check_pdf_resources(page: &craft_pdf::syntax::page::Page<'_>) -> Result<()> {
    use craft_pdf::syntax::object::{Object, Stream};
    let mut pending = vec![(Object::Dict(page.raw().clone()), 0usize)];
    let mut seen = std::collections::HashSet::new();
    let mut objects_left = 4096usize;
    let mut fonts_left = 16usize;
    let mut font_bytes_left = 32 * 1024 * 1024usize;
    while let Some((object, depth)) = pending.pop() {
        if depth > 32 {
            return Err(error("PDF resource nesting exceeds preview budget"));
        }
        let dict = match object {
            Object::Dict(dict) => dict,
            Object::Stream(stream) => stream.dict().clone(),
            Object::Array(array) => {
                if !seen.insert((array.data().as_ptr() as usize, array.data().len())) {
                    continue;
                }
                for child in array.iter::<Object<'_>>() {
                    objects_left = objects_left
                        .checked_sub(1)
                        .ok_or_else(|| error("PDF resource graph exceeds preview budget"))?;
                    pending.push((child, depth + 1));
                }
                continue;
            }
            _ => continue,
        };
        if !seen.insert((dict.data().as_ptr() as usize, dict.data().len())) {
            continue;
        }
        if dict.len() > objects_left {
            return Err(error("PDF resource graph exceeds preview budget"));
        }
        for key in dict.keys() {
            objects_left = objects_left
                .checked_sub(1)
                .ok_or_else(|| error("PDF resource graph exceeds preview budget"))?;
            if matches!(key.as_ref(), b"FontFile" | b"FontFile2" | b"FontFile3") {
                let stream = dict
                    .get::<Stream<'_>>(&key)
                    .ok_or_else(|| error("PDF font program is invalid"))?;
                let decoded = stream
                    .decoded_within(font_bytes_left.min(8 * 1024 * 1024))
                    .map_err(|_| error("PDF font program is invalid or exceeds preview budget"))?;
                // decoded_within does not cap unfiltered borrowed stream data.
                if decoded.len() > 8 * 1024 * 1024 {
                    return Err(error("PDF font program exceeds 8 MiB preview budget"));
                }
                font_bytes_left = font_bytes_left
                    .checked_sub(decoded.len())
                    .ok_or_else(|| error("PDF fonts exceed preview budget"))?;
            }
            if let Some(child) = dict.get::<Object<'_>>(&key) {
                if matches!(key.as_ref(), b"Font" | b"DescendantFonts") {
                    let count = match &child {
                        Object::Dict(fonts) => fonts.len(),
                        Object::Array(fonts) => fonts.raw_iter().take(17).count(),
                        _ => 1,
                    };
                    fonts_left = fonts_left
                        .checked_sub(count)
                        .ok_or_else(|| error("PDF has too many font references for a preview"))?;
                }
                pending.push((child, depth + 1));
            }
        }
    }
    Ok(())
}

fn pdf(bytes: &[u8], long_edge: u32) -> Result<DynamicImage> {
    if !bytes.starts_with(b"%PDF-") {
        return Err(MediaError::Unsupported(
            "expected a PDF-compatible document".into(),
        ));
    }
    let tail = bytes
        .get(bytes.len().saturating_sub(1024)..)
        .unwrap_or_default();
    if !tail.windows(5).any(|window| window == b"%%EOF") {
        return Err(error("PDF is incomplete or missing its end marker"));
    }
    let options = craft_pdf::OpenOptions {
        max_input_bytes: MAX_INPUT,
        decode_limits: craft_pdf::syntax::DecodeLimits::default()
            .with_max_decoded_stream(8 * 1024 * 1024)
            .with_max_decoded_image(96 * 1024 * 1024)
            .with_max_image_pixels(24_000_000),
    };
    let document = craft_pdf::open_slice_with_options(bytes, None, &options).map_err(error)?;
    let page = document
        .pages()
        .first()
        .ok_or_else(|| error("PDF has no pages"))?;
    check_pdf_resources(page)?;
    let (width, height) = page.render_dimensions();
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(error("PDF has invalid page dimensions"));
    }
    let scale = (long_edge.clamp(1, 1920) as f32 / width.max(height)).min(1.0);
    let options = craft_pdf::raster::Options {
        scale: [scale; 2],
        background: [255; 4],
        max_coverage: Some(16_000_000),
        ..Default::default()
    };
    let settings = craft_pdf::interpret::InterpreterSettings {
        jbig2_work: 16_000_000,
        ..Default::default()
    };
    let raster = craft_pdf::raster::render(
        page,
        &craft_pdf::raster::RenderCache::new(),
        &settings,
        &options,
    )
    .map_err(error)?;
    if !raster.diagnostics.is_empty() {
        return Err(error(
            "PDF preview omitted unsupported or budget-limited content",
        ));
    }
    let pixels = image::RgbaImage::from_raw(raster.width, raster.height, raster.bytes().to_vec())
        .ok_or_else(|| error("PDF raster length mismatch"))?;
    // An opaque white background makes premultiplied pixels ordinary RGBA.
    Ok(DynamicImage::ImageRgba8(pixels))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    fn vector_pdf(box_size: &str, rotate: u32) -> Vec<u8> {
        let content = "1 0 0 rg 0 0 200 100 re f";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {box_size}] /Rotate {rotate} /Resources << >> /Contents 4 0 R >>"),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        ];
        pdf_objects(&objects)
    }

    fn pdf_objects(objects: &[String]) -> Vec<u8> {
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{object}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").bytes());
        }
        out.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .bytes(),
        );
        out
    }

    #[test]
    fn first_pdf_page_is_bounded_rotated_and_opaque() {
        let bytes = vector_pdf("200 100", 90);
        let prepared = crate::prepare_media(&bytes, "pdf", 100).unwrap();
        assert_eq!((prepared.width, prepared.height), (None, None));
        assert_eq!(
            (prepared.thumbnail.width, prepared.thumbnail.height),
            (50, 100)
        );
        let pixels = image::load_from_memory(&prepared.thumbnail.bytes)
            .unwrap()
            .to_rgb8();
        let red = pixels.get_pixel(25, 50).0;
        assert!(red[0] > 240 && red[1] < 10 && red[2] < 10);
        let ai = crate::prepare_media(&bytes, "ai", 100).unwrap();
        assert_eq!(ai.format, "ai");
    }

    #[test]
    fn pdf_font_budget_covers_inherited_and_nested_resources() {
        for nested in [false, true] {
            for count in [1, 17] {
                let fonts = (0..count)
                    .map(|n| {
                        format!("/F{n} << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> ")
                    })
                    .collect::<String>();
                let font_resources = format!("<< /Font << {fonts} >> >>");
                let inherited = if nested {
                    "<< /XObject << /Nested 5 0 R >> >>".to_owned()
                } else {
                    font_resources.clone()
                };
                let objects = [
                    "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
                    format!("<< /Type /Pages /Kids [3 0 R] /Count 1 /Resources {inherited} >>"),
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >>".to_owned(),
                    "<< /Length 0 >>\nstream\n\nendstream".to_owned(),
                    format!("<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Resources {font_resources} /Length 0 >>\nstream\n\nendstream"),
                ];
                let prepared = crate::prepare_media(&pdf_objects(&objects), "pdf", 100);
                if count == 1 {
                    assert!(prepared.is_ok(), "{prepared:?}");
                } else {
                    assert!(prepared.unwrap_err().to_string().contains("too many font"));
                }
            }
        }
    }

    #[test]
    fn pdf_unfiltered_font_data_obeys_byte_budget() {
        let font = "x".repeat(8 * 1024 * 1024 + 1);
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /Font << /F << /Type /Font /Subtype /TrueType /FontDescriptor << /FontFile2 5 0 R >> >> >> >> /Contents 4 0 R >>".to_owned(),
            "<< /Length 0 >>\nstream\n\nendstream".to_owned(),
            format!("<< /Length {} >>\nstream\n{font}\nendstream", font.len()),
        ];
        let error = crate::prepare_media(&pdf_objects(&objects), "pdf", 100).unwrap_err();
        assert!(error.to_string().contains("font program exceeds 8 MiB"));
    }

    #[test]
    fn pdf_shared_program_does_not_bypass_font_copy_budget() {
        // Distinct font dictionaries can cause the backend to retain a separate
        // copy of the same program. Count references, not only stream identities.
        let fonts = (0..17).map(|n| format!("/F{n} << /Type /Font /Subtype /TrueType /FontDescriptor << /FontFile2 5 0 R >> >> ")).collect::<String>();
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /Font << {fonts} >> >> /Contents 4 0 R >>"),
            "<< /Length 0 >>\nstream\n\nendstream".to_owned(),
            "<< /Length 4 >>\nstream\nfont\nendstream".to_owned(),
        ];
        let error = crate::prepare_media(&pdf_objects(&objects), "pdf", 100).unwrap_err();
        assert!(error.to_string().contains("too many font"));
    }

    #[test]
    fn malformed_documents_and_missing_renditions_are_errors() {
        for hint in ["pdf", "ai", "pcraft", "vectorcraft", "ecproj"] {
            for bytes in [
                b"".as_slice(),
                b"%PDF-1.7\ninvalid",
                b"{\"format\":\"vectorcraft\",\"version\":999}",
            ] {
                assert!(crate::prepare_media(bytes, hint, 256).is_err());
            }
        }
        let bytes = vector_pdf("200 100", 0);
        for end in [1, 8, 30, bytes.len() / 2] {
            assert!(crate::prepare_media(&bytes[..end], "pdf", 256).is_err());
        }
    }

    #[test]
    fn vectorcraft_saved_image_then_pdf_fallback() {
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            20,
            10,
            image::Rgb([10, 120, 200]),
        ))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
        let pdf = vector_pdf("200 100", 0);
        for (image_bytes, expected_size) in [
            (png.as_slice(), (20, 10)),
            (b"broken PNG".as_slice(), (100, 50)),
        ] {
            let mut document = format!(r#"{{"format":"vectorcraft","version":4,"preview":{{"mime":"image/png","blob":[0,{}]}},"pdf":{{"mime":"application/pdf","blob":[{},{}]}},"document":{{}},"images":{{}}}}"#, image_bytes.len(), image_bytes.len(), pdf.len()).into_bytes();
            document.extend(b"\n\0VCBLOBS\0");
            document.extend(image_bytes);
            document.extend(&pdf);
            let prepared = crate::prepare_media(&document, "vectorcraft", 100).unwrap();
            assert_eq!(prepared.format, "vectorcraft");
            assert_eq!((prepared.width, prepared.height), (None, None));
            assert_eq!(
                (prepared.thumbnail.width, prepared.thumbnail.height),
                expected_size
            );
        }
    }

    #[test]
    fn photocraft_uses_saved_thumbnail_with_true_canvas_dimensions() {
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            20,
            10,
            image::Rgb([30, 90, 150]),
        ))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
        let manifest = br#"{"format_version":1,"generator":"synthetic","document":{"id":1,"name":"Synthetic","size":{"width":800,"height":400},"resolution_dpi":72.0,"mode":"Rgb","depth":"U8","layers":[]},"thumbnail":"thumb.png"}"#;
        let mut zip = craft_zip::Writer::new();
        zip.add("manifest.json", manifest, craft_zip::Compression::Stored)
            .unwrap();
        zip.add("thumb.png", &png, craft_zip::Compression::Stored)
            .unwrap();
        let prepared = crate::prepare_media(&zip.finish().unwrap(), "pcraft", 256).unwrap();
        assert_eq!(prepared.format, "pcraft");
        assert_eq!((prepared.width, prepared.height), (Some(800), Some(400)));
        assert_eq!(
            (prepared.thumbnail.width, prepared.thumbnail.height),
            (20, 10)
        );
    }
}
