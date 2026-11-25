//! Saved canvas PNG extraction from the observed CSFCHUNK `.clip` container.
//!
//! Format evidence and limits: `docs/clip-format.md`. SQLite wire primitives
//! come from craft-sqlite and <https://www.sqlite.org/fileformat.html>.
//! No SQL is executed, external layer blocks are not decoded, and no files are opened.
use crate::{MediaError, Result};
use craft_sqlite::{
    btree_cell_offset, decode_signed_be_int, local_payload_len, parse_btree_page_header,
    read_varint, serial_type_len, split_record, PayloadKind,
};
use std::collections::HashSet;

const MAX_DATABASE: usize = 64 * 1024 * 1024;
const MAX_RECORD: usize = 32 * 1024 * 1024;
const MAX_PAGES: usize = 16_384;
const MAX_CELLS: usize = 100_000;

/// The saved image's original PNG bytes and the source canvas dimensions.
#[derive(Debug)]
pub struct ClipPreview {
    pub png: Vec<u8>,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub preview_width: u32,
    pub preview_height: u32,
}

fn error(message: impl std::fmt::Display) -> MediaError {
    MediaError::Decode(format!("CLIP preview: {message}"))
}

fn take(bytes: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    at.checked_add(len)
        .and_then(|end| bytes.get(at..end))
        .ok_or_else(|| error("truncated data or invalid offset"))
}

fn be32(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        take(bytes, at, 4)?.try_into().map_err(error)?,
    ))
}

fn be64(bytes: &[u8], at: usize) -> Result<u64> {
    Ok(u64::from_be_bytes(
        take(bytes, at, 8)?.try_into().map_err(error)?,
    ))
}

fn database_chunk(bytes: &[u8]) -> Result<&[u8]> {
    if bytes.len() > 512 * 1024 * 1024
        || take(bytes, 0, 8)? != b"CSFCHUNK"
        || be64(bytes, 8)? != bytes.len() as u64
        || be64(bytes, 16)? != 24
    {
        return Err(error("unsupported container header or file length"));
    }
    let mut at = 24usize;
    let mut database = None;
    let mut database_offset = None;
    for index in 0..100_000 {
        let tag = take(bytes, at, 8)?;
        let len = usize::try_from(be64(bytes, at + 8)?).map_err(error)?;
        let body = take(bytes, at + 16, len)?;
        match tag {
            b"CHNKHead" if index == 0 && len == 40 => {
                if be64(body, 0)? != 256 || be64(body, 16)? != 16 {
                    return Err(error("unsupported container version"));
                }
                database_offset = Some(be64(body, 8)?);
            }
            b"CHNKExta" if index > 0 => {} // Layer data is intentionally opaque.
            b"CHNKSQLi" if index > 0 && database.is_none() => {
                if len > MAX_DATABASE || database_offset != Some(at as u64) {
                    return Err(error("database exceeds limit or header offset disagrees"));
                }
                database = Some(body);
            }
            b"CHNKFoot" if index > 0 && len == 0 && at + 16 == bytes.len() => {
                return database.ok_or_else(|| error("missing database"));
            }
            _ => return Err(error("unsupported, duplicated or misplaced chunk")),
        }
        at = at
            .checked_add(16)
            .and_then(|n| n.checked_add(len))
            .ok_or_else(|| error("chunk length overflow"))?;
    }
    Err(error("too many chunks"))
}

/// A deliberately narrow table B-tree reader. Page and payload budgets are shared
/// across schema/canvas/preview reads, including overflow chains; cycles are errors.
struct Database<'a> {
    bytes: &'a [u8],
    page_size: usize,
    usable: usize,
    visited: HashSet<u32>,
    cells: usize,
    payload_bytes: usize,
}

impl<'a> Database<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self> {
        if take(bytes, 0, 16)? != b"SQLite format 3\0" {
            return Err(error("invalid database signature"));
        }
        let raw = u16::from_be_bytes(take(bytes, 16, 2)?.try_into().map_err(error)?);
        let page_size = craft_sqlite::page_size_from_raw(raw).map_err(error)?;
        let reserved = *bytes
            .get(20)
            .ok_or_else(|| error("short database header"))?;
        let usable = craft_sqlite::usable_page_size(page_size, reserved).map_err(error)?;
        if take(bytes, 18, 2)? != [1, 1]
            || take(bytes, 21, 3)? != [64, 32, 32]
            || be32(bytes, 44)? != 4
            || be32(bytes, 56)? != 1
            || !bytes.len().is_multiple_of(page_size as usize)
            || u64::from(be32(bytes, 28)?) * u64::from(page_size) != bytes.len() as u64
        {
            return Err(error(
                "unsupported database header (requires complete UTF-8 schema-4 database)",
            ));
        }
        Ok(Self {
            bytes,
            page_size: page_size as usize,
            usable: usable as usize,
            visited: HashSet::new(),
            cells: 0,
            payload_bytes: 0,
        })
    }

    fn page(&mut self, number: u32) -> Result<&'a [u8]> {
        if number == 0 || self.visited.len() >= MAX_PAGES || !self.visited.insert(number) {
            return Err(error("invalid, repeated or excessive database page"));
        }
        let at = usize::try_from(number - 1)
            .map_err(error)?
            .checked_mul(self.page_size)
            .ok_or_else(|| error("page offset overflow"))?;
        take(self.bytes, at, self.usable)
    }

    fn rows(&mut self, root: u32, max_rows: usize, max_record: usize) -> Result<Vec<Vec<u8>>> {
        let mut pending = vec![(root, 0usize)];
        let mut rows = Vec::new();
        while let Some((number, depth)) = pending.pop() {
            if depth > 32 {
                return Err(error("database tree too deep"));
            }
            let page = self.page(number)?;
            let header = parse_btree_page_header(page, number == 1).map_err(error)?;
            self.cells = self.cells.saturating_add(header.cells);
            if self.cells > MAX_CELLS {
                return Err(error("too many database cells"));
            }
            if header.kind == 5 {
                if pending.len().saturating_add(header.cells).saturating_add(1) > MAX_PAGES {
                    return Err(error("too many database children"));
                }
                pending.push((be32(page, header.header_offset + 8)?, depth + 1));
                for index in (0..header.cells).rev() {
                    let at = btree_cell_offset(page, &header, index).map_err(error)?;
                    let child = be32(page, at)?;
                    read_varint(page.get(at + 4..).ok_or_else(|| error("short table key"))?)
                        .map_err(error)?;
                    pending.push((child, depth + 1));
                }
            } else if header.kind == 13 {
                for index in 0..header.cells {
                    if rows.len() >= max_rows {
                        return Err(error("too many table rows"));
                    }
                    let at = btree_cell_offset(page, &header, index).map_err(error)?;
                    let cell = page.get(at..).ok_or_else(|| error("invalid cell"))?;
                    let (len, prefix) = read_varint(cell).map_err(error)?;
                    let (_, rowid_len) =
                        read_varint(cell.get(prefix..).ok_or_else(|| error("short row id"))?)
                            .map_err(error)?;
                    let len = usize::try_from(len).map_err(error)?;
                    self.payload_bytes = self.payload_bytes.saturating_add(len);
                    if len > max_record || self.payload_bytes > MAX_DATABASE {
                        return Err(error("record or total payload exceeds limit"));
                    }
                    let local =
                        local_payload_len(self.usable as u32, len as u64, PayloadKind::TableLeaf)
                            .ok_or_else(|| error("invalid local payload"))?
                            as usize;
                    let start = prefix + rowid_len;
                    let mut payload = take(cell, start, local)?.to_vec();
                    if local < len {
                        let mut next = be32(cell, start + local)?;
                        while payload.len() < len {
                            let overflow = self.page(next)?;
                            next = be32(overflow, 0)?;
                            let count = (len - payload.len()).min(self.usable - 4);
                            payload.extend_from_slice(take(overflow, 4, count)?);
                        }
                        if next != 0 {
                            return Err(error("overlong overflow chain"));
                        }
                    }
                    rows.push(payload);
                }
            } else {
                return Err(error("expected rowid table B-tree"));
            }
        }
        Ok(rows)
    }
}

struct Value<'a> {
    kind: u64,
    bytes: &'a [u8],
}

impl Value<'_> {
    fn integer(&self) -> Result<i64> {
        match self.kind {
            1..=6 => decode_signed_be_int(self.bytes).map_err(error),
            8 => Ok(0),
            9 => Ok(1),
            _ => Err(error("expected integer")),
        }
    }
    fn text(&self) -> Result<&str> {
        if self.kind < 13 || self.kind.is_multiple_of(2) {
            return Err(error("expected text"));
        }
        std::str::from_utf8(self.bytes).map_err(error)
    }
    fn dimension(&self) -> Result<u32> {
        let value = if self.kind == 7 {
            f64::from_be_bytes(self.bytes.try_into().map_err(error)?)
        } else {
            self.integer()? as f64
        };
        if !value.is_finite() || !(1.0..=1_000_000.0).contains(&value) || value.fract() != 0.0 {
            return Err(error("invalid pixel dimensions"));
        }
        Ok(value as u32)
    }
}

fn record(payload: &[u8]) -> Result<Vec<Value<'_>>> {
    let (mut header, mut body) = split_record(payload).map_err(error)?;
    let mut values = Vec::new();
    while !header.is_empty() {
        if values.len() >= 256 {
            return Err(error("too many record fields"));
        }
        let (kind, used) = read_varint(header).map_err(error)?;
        header = header
            .get(used..)
            .ok_or_else(|| error("short record header"))?;
        let len = usize::try_from(serial_type_len(kind).map_err(error)?).map_err(error)?;
        values.push(Value {
            kind,
            bytes: take(body, 0, len)?,
        });
        body = body.get(len..).ok_or_else(|| error("short record body"))?;
    }
    if !body.is_empty() {
        return Err(error("trailing record data"));
    }
    Ok(values)
}

fn field<'a, 'b>(values: &'a [Value<'b>], index: usize) -> Result<&'a Value<'b>> {
    values
        .get(index)
        .ok_or_else(|| error("missing record field"))
}

struct Table {
    root: u32,
    columns: Vec<String>,
}

impl Table {
    fn column(&self, name: &str) -> Result<usize> {
        self.columns
            .iter()
            .position(|column| column == name)
            .ok_or_else(|| error(format!("missing column {name}")))
    }
    fn parse(root: i64, sql: &str, name: &str) -> Result<Self> {
        let (prefix, declarations) = sql
            .split_once('(')
            .ok_or_else(|| error("unsupported table schema"))?;
        if prefix.split_whitespace().collect::<Vec<_>>() != ["CREATE", "TABLE", name] {
            return Err(error("unsupported table declaration"));
        }
        let declarations = declarations
            .trim()
            .strip_suffix(')')
            .ok_or_else(|| error("invalid table schema"))?;
        // Only observed bare column declarations. No SQL execution or general SQL parser.
        if !declarations.bytes().all(|b| {
            b.is_ascii_alphanumeric() || b.is_ascii_whitespace() || matches!(b, b'_' | b',')
        }) {
            return Err(error("unsupported column declaration"));
        }
        let mut columns = Vec::new();
        for declaration in declarations.split(',') {
            let column = declaration
                .split_whitespace()
                .next()
                .ok_or_else(|| error("empty column declaration"))?;
            if columns.len() >= 256 || columns.iter().any(|c| c == column) {
                return Err(error("duplicate or excessive columns"));
            }
            columns.push(column.to_owned());
        }
        let root = u32::try_from(root).map_err(error)?;
        if root <= 1 {
            return Err(error("invalid table root"));
        }
        Ok(Self { root, columns })
    }
}

/// Extract the saved PNG from a single-canvas CSFCHUNK file.
///
/// Missing/invalid previews, unsupported schemas and exhausted budgets are errors.
/// Source bytes are capped at 512 MiB, the database/total visited payload at 64 MiB,
/// each image record at 32 MiB and the preview at 24 million pixels. This validates
/// container, table and PNG header structure; the caller must still decode the PNG
/// with image allocation limits before displaying it. This is not a layer renderer.
pub fn extract_preview(bytes: &[u8]) -> Result<ClipPreview> {
    let mut db = Database::new(database_chunk(bytes)?)?;
    let mut canvas = None;
    let mut preview = None;
    for row in db.rows(1, 1024, 64 * 1024)? {
        let fields = record(&row)?;
        let name = field(&fields, 1)?.text()?;
        if name != "Canvas" && name != "CanvasPreview" {
            continue;
        }
        if field(&fields, 0)?.text()? != "table" || field(&fields, 2)?.text()? != name {
            return Err(error("expected physical canvas table"));
        }
        let table = Table::parse(
            field(&fields, 3)?.integer()?,
            field(&fields, 4)?.text()?,
            name,
        )?;
        let slot = if name == "Canvas" {
            &mut canvas
        } else {
            &mut preview
        };
        if slot.replace(table).is_some() {
            return Err(error("duplicate canvas table"));
        }
    }
    let canvas = canvas.ok_or_else(|| error("missing Canvas table"))?;
    let preview = preview.ok_or_else(|| error("missing CanvasPreview table"))?;
    // Multiple canvases have no established selection semantics in our corpus.
    let rows = db.rows(canvas.root, 1, 1024 * 1024)?;
    let row = rows.first().ok_or_else(|| error("empty Canvas table"))?;
    let fields = record(row)?;
    let canvas_id = field(&fields, canvas.column("MainId")?)?.integer()?;
    if canvas_id <= 0 || field(&fields, canvas.column("CanvasUnit")?)?.integer()? != 0 {
        return Err(error("unsupported canvas id or unit"));
    }
    let canvas_width = field(&fields, canvas.column("CanvasWidth")?)?.dimension()?;
    let canvas_height = field(&fields, canvas.column("CanvasHeight")?)?.dimension()?;
    let rows = db.rows(preview.root, 1, MAX_RECORD)?;
    let row = rows
        .first()
        .ok_or_else(|| error("missing saved canvas preview"))?;
    let fields = record(row)?;
    if field(&fields, preview.column("CanvasId")?)?.integer()? != canvas_id
        || field(&fields, preview.column("ImageType")?)?.integer()? != 1
    {
        return Err(error("unsupported preview type or canvas reference"));
    }
    let width = field(&fields, preview.column("ImageWidth")?)?.dimension()?;
    let height = field(&fields, preview.column("ImageHeight")?)?.dimension()?;
    let image = field(&fields, preview.column("ImageData")?)?;
    if image.kind < 12
        || !image.kind.is_multiple_of(2)
        || take(image.bytes, 0, 8)? != b"\x89PNG\r\n\x1a\n"
        || be32(image.bytes, 8)? != 13
        || take(image.bytes, 12, 4)? != b"IHDR"
        || be32(image.bytes, 16)? != width
        || be32(image.bytes, 20)? != height
        || u64::from(width) * u64::from(height) > 24_000_000
    {
        return Err(error("invalid, oversized or mismatched saved PNG"));
    }
    Ok(ClipPreview {
        png: image.bytes.to_vec(),
        canvas_width,
        canvas_height,
        preview_width: width,
        preview_height: height,
    })
}
