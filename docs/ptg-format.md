# Saved `.ptg` canvas previews (D110)

`icatalog-media::ptg::extract_preview` extracts the saved full-canvas PNG from
the observed ArtRage Vitae container. Import, Update and Generate Previews share
this adapter. No layers are rendered, external paths followed or originals changed.

## Evidence

The owner supplied `ArtRage Vitae.ptg` and its PNG export on 2026-11-26 in the
sibling sample repository. The producer record says `ArtRage Vitae`; its exact
application build is unknown. Only these supplied files were studied, without
examining proprietary code or operating the application. Normal tests generate
original synthetic chunk/PNG fixtures. Real artwork remains in the optional
[sample corpus](media-corpus.md), with unresolved redistribution provenance.

| File | SHA-256 |
|---|---|
| Source PTG (7,354,870 bytes) | `78dd82a448d10f18d611b02f3b6e363497ab960cf7ac368b5626897f6a09db27` |
| Export PNG (1,125,811 bytes) | `cb101db5a1c7adf4fdbe1090fbb2bb647987fac980f3cb2dfbe198296b5f22c0` |
| Saved canvas PNG (1,156,780 bytes) | `df153c4520bc4cf4f0f27746e05fe55b2fe9f89a12e78983f3552779241d4ae8` |

Canvas, saved PNG and export are all **1440 × 876**. Fully decoded comparison
against the export gives mean absolute RGBA difference **0.0646 / 255**; the
encoded PNGs are not identical. This establishes a useful rendition for this
sample, not universal colour fidelity or coverage of other application versions.

## Observed layout and accepted subset

The exact 46-byte prefix is
`Ambient Design Project file. \r\nArtRage 2\r\n\0\0\x03\0`.
The ASCII text and line endings occupy 42 bytes, followed by `00 00 03 00`.
The reader matches that complete literal; no broader version scheme is inferred.

From offset 46, every top-level record has a little-endian u32 tag, a
little-endian u64 payload length, then exactly that many payload bytes. The
sample has 63 such records ending exactly at EOF, without padding.

| Offset | Tag | Payload | Interpretation |
|---|---|---|---|
| 46 | `0xff003002` | 36 bytes at 58 | UTF-16LE `ArtRage Vitae` plus NUL, then u32 LE width 1440 and height 876 |
| 94 | `0xff003081` | 36,629 bytes at 106 | 200 × 122 thumbnail PNG; not used |
| 36,735 | `0xff003b00` | 1,156,780 bytes at 36,747 | Full-canvas PNG, selected rendition |

Other records include nested PNGs of 400 × 400 and 200 × 200 dimensions. They
are not alternative canvas images. Only the top-level `0xff003b00` payload is
selected; no signature scanning or nested chunk recursion is used. Unknown
record payloads are skipped by length, not decoded. All top-level lengths must
fit the file, including records after the selected preview. Nested content is
opaque and is not validated as a complete painting document.

Exactly one canvas record and one saved-canvas record are required. The canvas
record must have exactly the observed UTF-16LE producer string, terminator and
two dimensions. The PNG must match the canvas dimensions exactly; reduced
full-canvas renditions and other producers need further sample evidence. Missing
full-canvas images fail normally even if a small thumbnail is present. Saved
pixels can be stale; no reliable freshness marker has been established.

## Bounds and reuse

The current craft-libs source and issue search found no ArtRage/PTG reader
(main `39c7c75c52aca67716e5b4bb46dd1196ae1943cc`, checked 2026-11-26).
Its `Ptg` references concern Excel formula tokens. This small byte-framing
adapter stays local following D106/D108; the shared-library preference in
upstream's agent guide can be addressed in a separate upstream contribution.
No code was copied and no dependency or Craft revision was changed. Existing
PNG decoding, white-background composition and JPEG generation are reused.

- Input at most 512 MiB; at most 10,000 top-level records.
- Checked lengths/slices; no input-driven recursion, paths or chunk allocations.
- Selected PNG at most 64 MiB, at most 24 million pixels.
- Canvas edges positive and at most one million pixels; PNG must equal canvas.
- Extractor probes PNG geometry; the adapter and tools fully decode before use,
  with the existing image allocation limits. Opaque chunks have no checksum here.

These are component bounds, not process isolation or a total time/memory bound.
Errors preserve the last good catalogue data through the existing commands.
Generator 3, schema v8 and sync protocol v2 stay unchanged because PTG files
were previously skipped. Other versions and native NAS/browser qualification
remain outstanding.

## Verification

Synthetic media tests cover missing/duplicate chunks, wrong headers/producers,
truncation, length overflow, chunk/pixel/byte budgets, corrupt pixels, unrelated
nested images and transparent pixels. The engine test exercises Import, Update,
Generate Previews, annotation preservation, failed updates and offline viewing.
The optional corpus case verifies source/export/saved-image hashes and geometry,
with mean RGBA difference limited to 0.1/255.

Extract the original saved PNG to a new file and compare the export:

```sh
task media:ptg-preview -- 'samples/ArtRage Vitae.ptg' /tmp/ptg-preview.png 'samples/ArtRage Vitae.png'
```

The tool refuses existing output files. Catalogue thumbnails and previews become
JPEGs at most 256/1920 pixels on the long edge without upscaling.
