# craft-libs dependency notices

This directory accompanies iCatalog's craft-libs integration. The copied files
are unmodified; `SOURCE.json` records their original paths, revisions and SHA-256
hashes. No font binaries, artwork or test photographs are copied here.

- craft-libs: <https://github.com/storytold/craft-libs>, revision
  `249a41bd9306a1b34330ac94e596a1a8b881378a`, MIT OR Apache-2.0.
- egui fork: <https://github.com/storytold/egui>, revision
  `6d518fc799681571963842f95b55a6b68431b15a`, MIT OR Apache-2.0.
- winit fork: <https://github.com/storytold/winit>, revision
  `a517b3846ee8457d84ee4975110bfda6797c44dc`, Apache-2.0.
- ICU/Unicode data used by craft-i18n: Unicode License v3, copied from the
  `icu_locale_core` 2.3.0 published crate.

`pdf-backend/` retains the patched Hayro and Vello licence texts. It also includes
the Foxit/PDFium standard-font and Adobe CMap BSD notices, and the CC0 notice for
the embedded CMYK profile. `egui/fonts/` retains the default-font notices,
including Hack, Ubuntu, Noto Emoji and the emoji icon font used for egui icons.
The craft-cms generated profiles are CC0, documented in upstream attribution.

The upstream `NOTICE` and `ATTRIBUTION.md` are preserved in full for provenance;
they describe the entire upstream repository, including optional components and
test assets that iCatalog does not distribute. Their presence does not mean that
all those components are linked. This bundle covers the craft-libs integration
and its fork/embedded-asset notices, not an exhaustive inventory of every
iCatalog dependency. CLI-only distributions carry the same bundle as desktop.

Distribution locations:

- NAS and Windows archives: `licenses/craft-libs/` beside the CLI.
- macOS archive: `licenses/craft-libs/` beside the CLI, and
  `iCatalog.app/Contents/Resources/licenses/craft-libs/` so moving the app keeps
  its notices.
- Linux packages and Docker image: `/usr/share/licenses/icatalog/craft-libs/`.

On dependency updates, recopy the relevant upstream notices, update provenance
and hashes, and run `task release:test`. Historical release recovery remains
supported: source predating craft-libs can be packaged without this directory;
source with a craft dependency must provide it.
