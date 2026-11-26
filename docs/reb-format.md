# Saved `.reb` canvas previews (D108)

The bounded reader in `icatalog-media::reb::extract_preview` extracts the saved
`canvas.png` from the observed ZIP container. Import, Update and Generate Previews
use it through the existing shared adapter. No layers are rendered, original
files changed, or archive paths extracted to disk.

## Evidence

The owner supplied `Rebelle Pro v8.reb` and its PNG export on 2026-11-26 in the
local sample repository. `artwork.xml` identifies application version 8.3.4 and
file format 8. Only these supplied files were inspected; no proprietary app,
installation or source code was examined. Normal tests generate original
synthetic ZIP/XML/PNG fixtures in Rust. Artwork provenance/licensing remains
explicitly unresolved in the optional [sample corpus](media-corpus.md).

| File | SHA-256 |
|---|---|
| Source `.reb` (9,418,896 bytes) | `1e722ae8f44c22926afc753aa75d5403c1b96227d62582aa93ccad8208a778b6` |
| Export PNG (5,811,165 bytes) | `c0b8eedfe7b85039b021f25c1f325dd80d51e7d02bb456ca76ca5b4a92962928` |
| Saved `canvas.png` (2,971,111 bytes) | `1c075fa52eede7a69632d2503e7737819234eda17e6e54fa98a6b2ba79bc5542` |

Canvas and export dimensions are **2339 × 1654**; the saved preview is
**1024 × 724**. Full decoding and comparison with a Lanczos3-resized export gives
mean absolute RGBA difference **0.9057 / 255**. This supports this sample only;
it is not a guarantee of colour fidelity or coverage of other application versions.

## Observed structure and accepted subset

The file is a standard ZIP with DEFLATE entries. The root `artwork.xml` has an
`aquarelle_artwork` element with `file_format_version='8'`. Its one direct `canvas`
child declares `width='2339.000000'`, `height='1654.000000'`, `units='1'` and
`safe_boundary='0'`. The reader requires format 8 and units 1, positive integral
dimensions up to one million per edge, and an absent or zero safe boundary.
The meaning of other units/boundaries has not been established. Unknown metadata
is ignored; duplicate attributes/canvases, namespaces, DTDs and malformed XML fail.

Only the root `canvas.png` is a preview candidate. The sample also has layer,
paper, granulation and reflection images plus opaque data entries. Those are
not preview alternatives and are never decompressed by this extractor. In
particular, `layer0.png` is canvas-sized but cannot stand in for a rendered canvas.
Paths in XML are never followed. No arbitrary PNG signature scan is performed.

The saved PNG must have positive dimensions, at most 24 million pixels, no edge
larger than the source canvas, and matching aspect ratio allowing one reduced
pixel of rounding. The extractor validates its header; the media adapter and
developer tools fully decode it before using it. Catalogue JPEGs composite
transparency over white, using at most 256/1920 pixels without upscaling.

## Craft reuse and bounds

Reuse `craft-zip` and `craft-xml` at the existing D105 revision
`249a41bd9306a1b34330ac94e596a1a8b881378a`. The inspected upstream main tree had
no Rebelle-specific crate. This local adapter follows the existing CLIP approach;
an upstream reusable facade would be a separate contribution. No Craft revision
is moved and no new transitive package is introduced.

- Source: 512 MiB. ZIP: 10,000 entries, 8 MiB central directory, 1024-byte names.
  Structural preflight precedes indexing. Duplicate names and overlapping entries
  fail. Selected entries receive full ZIP checksum and bounded inflation checks.
- XML: 1 MiB, depth 32, 20,000 nodes, 10,000 elements, 128 attributes per element
  and 40,000 total attributes. DTDs and unknown entities are rejected.
- Saved PNG bytes: 64 MiB. Total successful ZIP reads: 65 MiB. Image decoding
  also uses the existing allocation limits. Unrelated layers are not decoded.

These are component caps, not process isolation or a total time/memory guarantee.
Missing/corrupt/oversized previews are per-file failures that preserve the last
good catalogue data. No preview freshness indicator is known. Other format
versions, units and boundary modes need more evidence; layer rendering is absent.
Generator 3, schema v8 and sync protocol v2 remain unchanged because `.reb` was
previously skipped. Native NAS/browser qualification remains outstanding (D105).

## Verification

`task test:media` exercises extraction, XML/ZIP bounds, ambiguity, corruption,
truncation and mutation safety. The generated engine regression covers Import,
Update, Generate Previews, preserved edits/last good pixels and offline viewing.
`task test:corpus` checks both supplied pairs against their manifest.

To extract original saved PNG bytes to a new file and compare the export:

```sh
task media:reb-preview -- 'samples/Rebelle Pro v8.reb' /tmp/reb-preview.png 'samples/Rebelle Pro v8.png'
```

The tool refuses to overwrite existing output. Actual PNG pixels remain 1024 ×
724 for the supplied file; neither extraction nor catalogue preview generation
reconstructs a full-resolution render from its layers.
