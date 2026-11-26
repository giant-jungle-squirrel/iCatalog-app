//! Saved PTG canvas PNGs from the observed ArtRage Vitae container.
//! See `docs/ptg-format.md`; this is not a layer renderer.
use crate::{MediaError, Result};

const HEADER: &[u8] = b"Ambient Design Project file. \r\nArtRage 2\r\n\0\0\x03\0";
const CANVAS: u32 = 0xff00_3002;
const PREVIEW: u32 = 0xff00_3b00;

/// Saved PNG bytes and source geometry, without decoding layers.
#[derive(Debug)]
pub struct PtgPreview {
    pub png: Vec<u8>,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub preview_width: u32,
    pub preview_height: u32,
}

fn error(message: &str) -> MediaError {
    MediaError::Decode(format!("PTG preview: {message}"))
}

fn take<'a>(bytes: &mut &'a [u8], length: usize) -> Result<&'a [u8]> {
    let data = bytes
        .get(..length)
        .ok_or_else(|| error("truncated chunk"))?;
    *bytes = bytes
        .get(length..)
        .ok_or_else(|| error("truncated chunk"))?;
    Ok(data)
}

fn u32_le(bytes: &mut &[u8]) -> Result<u32> {
    let value = take(bytes, 4)?
        .try_into()
        .map_err(|_| error("short integer"))?;
    Ok(u32::from_le_bytes(value))
}

fn dimensions(mut bytes: &[u8]) -> Result<(u32, u32)> {
    // Only this producer/header layout is evidenced. Do not infer dimensions
    // from arbitrary application strings or trailing bytes of unknown layouts.
    const PRODUCER: &[u8] = b"A\0r\0t\0R\0a\0g\0e\0 \0V\0i\0t\0a\0e\0\0\0";
    if take(&mut bytes, PRODUCER.len())? != PRODUCER || bytes.len() != 8 {
        return Err(error("unsupported canvas header (expected ArtRage Vitae)"));
    }
    let width = u32_le(&mut bytes)?;
    let height = u32_le(&mut bytes)?;
    if !(1..=1_000_000).contains(&width) || !(1..=1_000_000).contains(&height) {
        return Err(error("canvas dimensions exceed limits"));
    }
    Ok((width, height))
}

/// Extract the one top-level saved-canvas PNG from the observed header version.
/// Checks all top-level chunk boundaries, uniqueness and PNG header geometry.
/// Callers must fully decode the PNG before displaying it (the shared adapter does).
pub fn extract_preview(bytes: &[u8]) -> Result<PtgPreview> {
    if bytes.len() > 512 * 1024 * 1024 {
        return Err(error("source exceeds 512 MiB limit"));
    }
    let mut remaining = bytes
        .strip_prefix(HEADER)
        .ok_or_else(|| error("unsupported signature or header version"))?;
    let mut canvas = None;
    let mut saved = None;
    let mut chunks = 0u32;
    while !remaining.is_empty() {
        chunks += 1;
        if chunks > 10_000 {
            return Err(error("too many top-level chunks"));
        }
        let tag = u32_le(&mut remaining)?;
        let length = u64::from_le_bytes(
            take(&mut remaining, 8)?
                .try_into()
                .map_err(|_| error("short chunk length"))?,
        );
        let length = usize::try_from(length).map_err(|_| error("chunk length overflow"))?;
        let payload = take(&mut remaining, length)?;
        match tag {
            CANVAS => {
                if canvas.is_some() {
                    return Err(error("duplicate canvas header"));
                }
                canvas = Some(dimensions(payload)?);
            }
            PREVIEW => {
                if saved.is_some() {
                    return Err(error("duplicate saved canvas"));
                }
                if payload.len() > 64 * 1024 * 1024 {
                    return Err(error("saved PNG exceeds 64 MiB limit"));
                }
                saved = Some(payload);
            }
            // Nested layer/tool images and the small thumbnail are not canvases.
            _ => {}
        }
    }
    let (canvas_width, canvas_height) = canvas.ok_or_else(|| error("missing canvas header"))?;
    let png = saved.ok_or_else(|| error("no saved canvas PNG; layer rendering is unsupported"))?;
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(error("saved canvas is not PNG data"));
    }
    let info = crate::probe(png)?;
    let preview_width = u32::try_from(info.width).map_err(|_| error("invalid PNG width"))?;
    let preview_height = u32::try_from(info.height).map_err(|_| error("invalid PNG height"))?;
    if (preview_width, preview_height) != (canvas_width, canvas_height)
        || u64::from(preview_width) * u64::from(preview_height) > 24_000_000
    {
        return Err(error(
            "saved PNG dimensions disagree with canvas or exceed limits",
        ));
    }
    Ok(PtgPreview {
        png: png.to_vec(),
        canvas_width,
        canvas_height,
        preview_width,
        preview_height,
    })
}
