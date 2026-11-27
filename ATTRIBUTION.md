# Third-party source attribution

iCatalog's legacy catalogue research remains clean-room. No iView, Expression
Media or Phase One source code or assets are included.

| Component | Origin | Licence | Local use |
|---|---|---|---|
| `tiny_http` 0.12.0 | [tiny-http/tiny-http](https://github.com/tiny-http/tiny-http), crates.io release 0.12.0 | MIT OR Apache-2.0; both licence texts retained in `vendor/tiny_http/` | Vendored HTTP server dependency, patched for bounded rejected-request handling and fallible Rustls TLS configuration. See [patch notes](vendor/tiny_http/ICATALOG-PATCH.md). |
| Craft media and GUI crates | [storytold/craft-libs](https://github.com/storytold/craft-libs/tree/249a41bd9306a1b34330ac94e596a1a8b881378a), exact revision `249a41bd9306a1b34330ac94e596a1a8b881378a` | MIT OR Apache-2.0; texts and upstream notices retained in `licenses/craft-libs/` | RAW preview/sensor decode and basic development, metadata/TIFF, PSD/KRA/ORA/Affinity/EPS/PDF/Craft document renditions; native docking/workspace components, widget palettes, toolbar fitting/overflow, icon buttons, section layout and queued pickers. See [coverage](docs/media-support.md). |
| Coordinated egui and winit forks | [storytold/egui](https://github.com/storytold/egui/tree/6d518fc799681571963842f95b55a6b68431b15a), [storytold/winit](https://github.com/storytold/winit/tree/a517b3846ee8457d84ee4975110bfda6797c44dc) | egui MIT OR Apache-2.0; winit Apache-2.0 | Root patches for the native egui 0.37 stack. Default/monochrome font notices are retained. |
| Patched Hayro and Vello PDF backends | craft-libs revision above, root Cargo patches | MIT OR Apache-2.0; retained backend notices | First-page CPU rasterization; embedded Foxit/PDFium fonts and Adobe CMaps use BSD notices, the CMYK profile is CC0. No Poppler/system PDF renderer. |
| Unicode/ICU data | `icu_locale_core` 2.3.0, through craft-i18n | Unicode License v3, retained | Shared UI locale support. |
| `craft-sqlite` wire helpers | Same exact craft-libs revision above | MIT OR Apache-2.0; covered by the retained Craft licence texts | Bounded SQLite byte primitives for saved `.clip` canvas PNG extraction (D106); no SQL execution. |
| `craft-zip` and `craft-xml` | Same exact craft-libs revision above | MIT OR Apache-2.0; covered by the retained Craft licence texts | Bounded ZIP/XML reads for saved `.reb` canvas PNG extraction (D108); no layer rendering or external file access. |
| `craft-plist` | Same exact craft-libs revision above | MIT OR Apache-2.0; covered by the retained Craft licence texts and upstream attribution | Bounded binary property-list reading for Procreate/ProCraft document geometry; existing craft-zip reads the saved QuickLook PNG (D111). No application code copied. |

D111 normal tests generate original binary plist/ZIP/PNG fixtures. Only the
owner-authorized ProCraft examples belong in the optional corpus; the private
third-party Procreate document, reference image and extracted artwork are excluded.

D108 fixtures in `icatalog-media/tests/reb.rs` and `icatalog-engine/tests/reb_import.rs` generate original synthetic pixels/XML/ZIP bytes under this repository’s MIT licence. The supplied REB/export pair stays in the optional sample corpus with unresolved provenance/licensing; see [REB evidence](docs/reb-format.md).

D106 test images and database fixtures in `icatalog-media/tests/clip.rs` and
`icatalog-engine/tests/clip_import.rs` are generated in Rust from original
synthetic pixels/schema data, under this repository's MIT licence. The owner's
local `.clip`/PNG pair lives in the optional separate sample repository (D107).
It is not published or embedded in normal tests; its manifest records unresolved
licensing/provenance and opt-in corpus expectations. See [evidence and hashes](docs/clip-format.md)
and [corpus setup](docs/media-corpus.md).

Other Rust dependencies are resolved by Cargo and retain their respective package
licences. This is not a complete software bill of materials. The
[Craft notice bundle](licenses/craft-libs/README.md) records source paths and
hashes and is included in desktop/NAS packages and Docker images. Upstream's full
attribution is retained for provenance; it lists optional assets and test corpora
that iCatalog does not ship. Embedded fonts, CMaps and colour profiles are used;
no third-party photographs or authoring-application artwork are included.
