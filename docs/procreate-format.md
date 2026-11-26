# Saved Procreate and ProCraft previews (D111)

`icatalog-media::procreate::extract_preview` reads the saved
`QuickLook/Thumbnail.png` from the observed `.procreate` and `.procraft`
containers. Import, Update and Generate Previews share this adapter. No layers
or video are rendered and no originals are changed.

## Evidence and permission boundary

On 2026-11-26 the owner supplied a real Procreate document with a JPEG reference,
and ProCraft 0.8.0 examples with a PNG export. Only the three ProCraft files are
authorized for committing to the optional sample repository. The private real
`Procreate.procreate` and `Procreate.jpg` (the request called the reference PNG)
are excluded, as are extracted copies of their artwork. Corpus ignore rules also
cover `Procreate.png`. No private bytes appear in generated tests. Source-code
study of either painting application was unnecessary; these observations come
from the supplied files. Commit permission does not establish a public artwork
licence; the corpus records NOASSERTION.

| Observation | Real Procreate (private, one sample) | ProCraft `.procreate` | ProCraft `.procraft` |
|---|---|---|---|
| File bytes | 157,865,125 | 19,956,380 | 19,956,382 |
| ZIP entries | 570 | 196 | 196 |
| Document version | 1 | 2 | 2 |
| Archived size | 1080 × 1920 | 2880 × 1920 | 2880 × 1920 |
| Orientation field | 4 | 1 | 1 |
| Displayed canvas / reference | 1920 × 1080 | 2880 × 1920 | 2880 × 1920 |
| Saved QuickLook PNG | 1024 × 576 | 1024 × 683 | 1024 × 683 |
| Saved PNG bytes | 1,083,799 | 809,871 | 809,871 |
| Mean RGBA difference from resized reference | 0.6251 / 255 | 0.4970 / 255 | 0.4970 / 255 |
| Tile entry suffix | `.chunk` | `.lz4` | `.lz4` |

Comparison fully decodes the PNG and reference and resizes the reference with
Lanczos3. This is a numerical comparison without ICC conversion, not a perceptual
or colour-management guarantee. The real reference is JPEG, so exact pixel
equality is not expected. The saved PNG already has display orientation: rotating
it again would be wrong for the observed real file.

Both ProCraft files contain byte-identical QuickLook PNGs and matching multisets
of 192 LZ4 tile payloads. UUIDs and elapsed-time metadata differ between saves.
The common preview envelope makes these useful extraction fixtures; it does
not establish full Procreate/ProCraft document or layer interoperability.

| Owner-authorized file / rendition | SHA-256 |
|---|---|
| `ProCraft_v080.procreate` | `cb7b8332ab914306fcd7eb60d10165c77946782844b6b764f11370b77632c779` |
| `ProCraft_v080.procraft` | `9d71e9a764401c387f9999f12211eee6bf12112711a53acb25f7d1fbb596f4ab` |
| `ProCraft v080.png` export | `fc610f284c5acdd7ef2f5bb15a6f4a1c69b7e8741b9aa532536ea09294de7f0f` |
| Shared saved QuickLook PNG | `795b118db64c62e2af5765f9442055e76cb723c1b651ceb71c70df008a524a3e` |

## Accepted subset

Both containers are ZIP archives, with the following exact, case-sensitive paths:

- `Document.archive`: binary `bplist00` property list with `$archiver` equal to
  `NSKeyedArchiver`, `$version` equal to 100000 and an `$objects` array whose first
  member is `$null`. `$top.root` is a UID into that array. The root's `$class` UID
  identifies a class whose `$classname` is `SilicaDocument`.
- The root's `size` UID identifies a `{width, height}` string of positive whole
  pixel dimensions. Only document versions 1/2 and unit key
  `SilicaDocumentArchiveUnitKey = 0` are accepted. Only observed orientations 1
  (unchanged axes) and 4 (swapped axes) are accepted. This is an observed subset,
  not an inferred general orientation enumeration.
- `QuickLook/Thumbnail.png`: the only selected image. No fallback to layer tiles,
  arbitrary nested PNGs or video. Preview axes must fit the displayed canvas and
  preserve its aspect ratio within one preview pixel of rounding.

UID resolution is limited to the necessary root/class/size links; the reader
does not instantiate objects or walk layer graphs. Missing or ambiguous metadata,
unknown versions/orientations, duplicate plist keys, duplicate/overlapping ZIP
entries, checksum failures and invalid geometry fail normally. Unread layer and
video payloads are opaque, so extraction does not certify the entire document.
No saved-preview freshness marker has been established.

## Bounds and integration

Reuse craft-zip and add craft-plist at the existing exact craft-libs pin. Neither
the pin nor any other dependency version is intentionally upgraded. The local
adapter supplies Procreate-specific keyed-archive interpretation.

- Source at most 512 MiB; at most 10,000 ZIP entries, 8 MiB central directory,
  and 1,024 bytes per entry name. Paths are never extracted to the filesystem.
- Document archive at most 4 MiB; plist at most 100,000 objects/expanded nodes,
  depth 32, and 8 MiB expanded data/text. Duplicate-key traversal is iterative.
- Saved PNG at most 64 MiB and 24 million pixels; canvas edges at most one
  million pixels. PNG geometry is checked before pixel decoding.
- ZIP decompressed reads have a combined 68 MiB budget. The adapter and tool
  fully decode PNG data with existing image allocation limits before use.

These are component budgets, not process isolation. Source dimensions describe
the displayed canvas; the smaller thumbnail dimensions are never substituted.
Both extensions record `procreate_quicklook` image provenance. Transparent saved
pixels are composed over white for catalogue JPEGs, without upscaling. No layer
rendering, schema/protocol change or generator bump is needed; these extensions
were previously skipped. Native NAS and wasm qualification remain open.

## Verification and extraction

Generated synthetic tests cover both extensions, metadata references, orientation,
ZIP/plist corruption, budgets, missing renditions and pixel decoding. Engine
regressions verify Import/Update/Generate, preserved annotations and last-good
pixels after failed updates, plus offline stored-preview viewing. The optional
corpus includes only the two ProCraft documents and their shared reference, with
exact hashes and a 0.6/255 RGBA comparison limit.

```sh
task media:procreate-preview -- samples/ProCraft_v080.procreate /tmp/procreate-preview.png 'samples/ProCraft v080.png'
task media:procreate-preview -- samples/ProCraft_v080.procraft /tmp/procraft-preview.png 'samples/ProCraft v080.png'
task test:corpus
```

Outputs are byte-exact saved PNGs; existing output paths are refused. Catalogue
JPEGs remain bounded to 256/1920 pixels. Wider Procreate versions, other `.procraft`
implementations, missing-preview files and additional orientations need evidence.
