# Media previews and craft-libs (D105)

iCatalog uses craft-libs commit
[`249a41bd9306a1b34330ac94e596a1a8b881378a`](https://github.com/storytold/craft-libs/tree/249a41bd9306a1b34330ac94e596a1a8b881378a)
(2026-11-23). This is an exact source revision, not the unreleased `v0.1.0`
tag advertised in upstream documentation. Cargo.lock records the resolved graph.

Import, Update and Generate Previews share the same `icatalog-media` adapter.
Saved images are preferred; supported formats can fall back to a basic conversion.
The result is an oriented JPEG: at most 256 pixels for grid thumbnails or 1920
pixels for stored previews, without upscaling saved raster images. New document
previews composite straight-alpha saved images over white; PSD merged colour
planes already carry their saved matte. Original files are never modified. There
are no RAW editing controls or development recipes.

| Family | Available path | Limits of coverage |
|---|---|---|
| JPEG, PNG, GIF, WebP, BMP, TIFF | Existing raster decode and EXIF orientation | TIFF remains a raster unless the catalogued extension identifies RAW |
| Camera RAW | Existing embedded-JPEG locator, then craft-raw preview extraction; sensor decode, bilinear demosaic, as-shot white balance and basic sRGB conversion if neither image decodes | Camera/container support depends on the pinned decoder; unsupported, oversized or damaged files are skipped. This is a catalogue rendition, not an authoring application's developed image |
| Photoshop PSD/PSB | Saved JPEG thumbnail resource; otherwise the saved merged RGB/grayscale composite (8/16/32-bit, raw/RLE/ZIP/ZIP-prediction compression) | No layer compositor; other colour modes need a usable saved thumbnail. Declared placeholder composites are rejected |
| Krita / OpenRaster | Saved merged PNG, then saved thumbnail | No layer rendering; a saved image must exist |
| Affinity `.afphoto`, `.afdesign`, `.afpub`, `.af` | Saved PNG preview recognized by craft-affinity | No layer rendering; acceptance of an extension does not establish coverage of every application generation. Source canvas dimensions remain unknown |
| PDF / PDF-compatible Illustrator | First-page render, including page crop/rotation, onto white | No embedded `/Thumb` extraction yet. Password-protected documents are unsupported. Reported omitted/budget-limited content rejects the result; unreported fidelity loss remains possible |
| EPS / older Illustrator | Saved TIFF or EPSI preview | No PostScript execution; source canvas dimensions remain unknown |
| PhotoCraft / VectorCraft | Saved composite or thumbnail; VectorCraft saved compatible PDF as fallback | Supported upstream envelope versions only; no document/layer renderer |
| EffectCraft | Envelope recognized, but no usable rendition is currently exposed | Skipped; no animation renderer |

ArtRage, Rebelle, Clip Studio Paint, InDesign and Procreate remain planned (D103).
There is no suitable preview facade for these in the adopted set. Application
editor, timeline, docking, GPU and writing APIs are not dependencies merely
because they exist upstream.

Source dimensions are stored only when the source supplies meaningful pixel
dimensions. PDF points and small embedded-preview dimensions are not substituted
for the original document's pixel dimensions. RAW preview colour information is
not attributed to the sensor data. Existing XMP seeding remains first-ingest-only;
Update preserves ratings, labels, keywords and other user annotations.

## Existing catalogues

Run **Update** on the machine with access to originals. Thumbnail generator 3
causes older thumbnails to be rebuilt even when file size, time and content hash
are unchanged. Asset identities and user annotations survive. Subsequent unchanged
updates take the normal fast path. Failed decoding preserves the last good asset
metadata and thumbnail.

Run **Generate Previews** separately to refresh stored previews. Its cache checks
both source content hash and `metadata_raw`'s `preview_generator/jpeg` marker,
containing `<generator-version>:<preview-hash>`. Missing, older or mismatched
markers trigger regeneration; corrupt blobs still trigger regeneration. A failed
conversion keeps the last good stored preview. Schema v8 and sync protocol v2 do
not change; older readers ignore this metadata. An older generator that changes
the preview hash invalidates the marker for the next new-generator run.

Replicas receive stored thumbnails/previews through existing sync. They do not
decode original media or acquire permission to perform media-derived writes.

## Dependency and resource policy

Only `icatalog-media` depends on the Craft media crates. The desktop uses
`craft-ui` icon buttons and section layout, plus `craft-dialogs` queued native
pickers. All catalogue actions still dispatch commands. The UI and test tools
use egui 0.37 with the coordinated egui/winit revisions in root Cargo.toml.
Optional Craft system-font discovery and upstream downloaded font assets are
disabled; egui bundled monochrome glyphs retain rating stars and the native font
fallback remains available.

PDF uses the same pinned hayro, hayro-syntax, hayro-interpret, hayro-jbig2,
hayro-cmap, hayro-postscript, vello_common and vello_cpu sources as upstream's
tested workspace. Consumer root patches are essential: dependency manifests do
not propagate their own workspace patches. Licensing and bundled font/profile
notices are retained in [the notice bundle](../licenses/craft-libs/README.md)
and included in release packages and container images.

The adapter caps source bytes at 512 MiB. PDF/Craft documents have a stricter
64 MiB input cap; Craft JSON is capped at 16 MiB, entries at 32 MiB, total decoded
data per read at 64 MiB, nesting at 64 and listed items at 100,000. RAW sensor
decode and PSD composite work are limited to 24 million samples/pixels; PSD
decoded channels are capped at 96 MiB. KRA/ORA saved-image entries have a 64 MiB cap. Raster decode also uses image allocation
limits. PDF limits ordinary decoded streams to 8 MiB, image expansion to 96 MiB,
individual images to 24 million pixels, and coverage/JBIG2 work to 16 million.
Before rendering, PDF resource preflight caps reachable objects at 4,096, nesting
at 32, font references at 16, individual font programs at 8 MiB and visited font
program bytes at 32 MiB. The reference cap also bounds repeated retained copies
of a shared font program (at most 16 × 8 MiB of program copies, plus parser/font structures);
the 32 MiB figure is not a total cache-memory cap. It follows parent/page
references conservatively, so related pages can cause an otherwise simple first
page to exceed the budget. Native escaped
decoder panics become normal per-file failures.

These are component limits, not process memory isolation or a total execution
deadline. Upstream PDF parsing/rendering has additional internal budgets and can
omit content without a diagnostic. High-resolution RAWs without a reduced decode
path may exceed the budget and need an embedded preview. A failed conversion does
not certify that the source is corrupt or unsupported by its authoring app.

Tests use generated synthetic documents: extraction preference, missing/damaged
preview fallbacks, RAW conversion, orientation, truncation/oversized inputs,
annotation-preserving Update, cache invalidation and offline stored-preview use.
Real-camera and application-version coverage remains to be expanded. Native
Linux x86-64/ARM64 musl and browser runtime qualification must be run before
claiming those platforms for this new dependency graph; RAW's Rayon use needs
particular browser review. This change does not establish wasm support.
