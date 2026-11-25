# Saved `.clip` canvas previews (D106)

This is a narrow, observation-based reader, not a complete Clip Studio Paint
format specification or layer renderer. The implementation is
`icatalog-media::clip::extract_preview`; Import, Update and Generate Previews use
it through the shared media adapter. Original files are never modified.

## Evidence and provenance

On 2026-11-25 the owner supplied `samples/Clip Studio Paint v1.clip` and a PNG
export for local testing. No proprietary application, installation or source
code was inspected. D107 moved the sample artwork and export into a separate local
repository, pinned by the optional `samples/` submodule; see [corpus setup](media-corpus.md).
They are not embedded in normal workspace tests or published as licensed fixtures.
Committed tests generate their own PNG pixels and SQLite databases in Rust.

| Local input/output | SHA-256 |
|---|---|
| `.clip` input | `a8985b7e031c09aa975ce51bf5a4a0bf9b8a6d4df219e5caec99d07dd387d197` |
| PNG export | `6d71404b232a9b567bf42e24249bbc13d26ed6f167b6fa26c05353814d28a7c7` |
| Extracted PNG | `2913fcbc63681ce277c306a7ab1d1768be9eae99e848f814036b7e9f2201e23e` |

The saved PNG is 1754 × 1240 (3,214,197 bytes); the canvas and export are
3508 × 2480. Comparing decoded RGBA channels after resizing the export with
Lanczos3 gives a mean absolute difference of 0.1887 on the 0–255 scale. This is
evidence for this sample, not proof of colour fidelity or other version coverage.

The adopted craft-libs pin remains `249a41bd9306a1b34330ac94e596a1a8b881378a`.
Its `craft-sqlite` crate supplies varint, record-header, cell-pointer, integer and
local-payload primitives. SQLite interpretation follows the
[SQLite database file format](https://www.sqlite.org/fileformat.html).
The inspected upstream main `2c81ed850319ef3504ce56d82379b346522af18e` had no
Clip Studio extractor. This app adapter does not represent an upstream addition;
a reusable upstream format facade can replace it in a separately reviewed change.

## Observed container

All container integers below are unsigned, big-endian 64-bit values. SQLite has
its own separately specified encoding.

| Offset | Observed meaning |
|---|---|
| `0x00` | Eight-byte signature `CSFCHUNK` |
| `0x08` | Total file length, 11,587,365 bytes in this sample |
| `0x10` | First chunk offset, 24 |
| `0x18` | `CHNKHead`, followed by payload length 40 |
| `0x28` | Observed header marker 256; broader version semantics unknown |
| `0x30` | Absolute offset of `CHNKSQLi`, `0x7cef05` |
| `0x38` | Opaque identifier length, 16; next 16 bytes skipped |
| `0x50` | First of four `CHNKExta` chunks, skipped by length |
| `0x7cef05` | `CHNKSQLi`, followed by payload length 3,399,680 |
| `0x7cef15` | SQLite database begins (`SQLite format 3` and NUL) |
| `0xb0cf15` | Final `CHNKFoot`, payload length zero |

Each chunk is an eight-byte tag, an eight-byte payload length and that many bytes
of payload. The reader requires one leading header, one matching database offset
and a final empty footer. Duplicate/unknown/misplaced chunks and truncation fail.
It follows lengths rather than scanning for image signatures in unrelated layers.

## Observed database tables

The sample uses 4096-byte pages, UTF-8 text and schema format 4. The reader locates
physical `Canvas` and `CanvasPreview` tables through SQLite's schema table.
It supports rowid table interior/leaf pages and checked overflow chains. It never
executes schema SQL, views, triggers or application statements. Column mapping
accepts the observed bare `CREATE TABLE` declarations; quoted names, expressions
and arbitrary SQL grammar are outside this implementation.

Required `Canvas` fields are `MainId`, `CanvasUnit`, `CanvasWidth` and
`CanvasHeight`. In the sample they are 1, 0, 3508 and 2480. Unit 0 is the only
accepted observed unit; dimensions must be finite, positive integral values.
Source dimensions come from this table, not from the reduced preview.

Required `CanvasPreview` fields are `CanvasId`, `ImageType`, `ImageWidth`,
`ImageHeight` and `ImageData`. The sample has canvas reference 1, type 1 and a PNG
blob whose IHDR dimensions agree with 1754 × 1240. The reader requires exactly
one canvas and exactly one preview referring to it. Unknown types, multiple
canvases/previews, absent saved images and mismatched dimensions are errors.
Animation/page selection, layer composition and preview freshness detection are
not implemented. Saved pixels can be stale if the authoring application saved
a stale preview; no freshness flag has been established from this sample.

## Bounds and verification

- Input: 512 MiB; database: 64 MiB; individual preview record: 32 MiB.
- Container: at most 100,000 chunks. Database: at most 16,384 visited pages,
  100,000 cells, tree depth 32, 1,024 schema rows and 256 columns per record.
- Total visited record payload: 64 MiB. Schema records: 64 KiB; canvas record:
  1 MiB. Repeated tree/overflow pages, missing pages and overlong chains fail.
- Saved PNG: at most 24 million pixels. Extraction checks the PNG header; the
  shared adapter and corpus tool fully decode it with existing allocation limits.
  Catalogue JPEGs composite transparency on white and never upscale the preview.

These component caps are not a total process-memory limit. SQLite reads allocate
only bounded record data; no native SQLite connection or temporary file is used
by the runtime extractor. `rusqlite` is a media **test-only** dependency for
creating synthetic database fixtures. No catalogue/schema/protocol change is
needed, and generator 3 stays unchanged because `.clip` was previously skipped.

Run `task test:media` and `task test:engine` for generated fixtures, including
overflow/interior pages, truncation, cycles, hostile lengths, ambiguity and
preserving annotations/last good catalogue pixels after failed decoding.

Run `task test:corpus` to verify the optional sample repository's manifest, hashes,
dimensions and export comparison. For local evidence, the developer tool extracts the original PNG into a **new**
file and optionally compares an export; it refuses to overwrite an existing path:

```sh
task media:clip-preview -- 'samples/Clip Studio Paint v1.clip' /tmp/clip-preview.png 'samples/Clip Studio Paint v1.png'
```

Only this supplied v1 sample has real-application validation. Newer application
versions, animation documents and differently structured files need more evidence.
Native NAS and browser qualification remains outstanding as in D105.
