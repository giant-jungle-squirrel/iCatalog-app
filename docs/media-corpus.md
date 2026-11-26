# Optional media sample repository (D107)

Real application documents and reference exports live in the separate
`media-format-samples` Git repository. iCatalog uses a pinned checkout under
`samples/` as an optional submodule. Generated fixtures remain in the normal
workspace tests; `task ci` does not download or require the real sample corpus.
The small, already licensed legacy oracle corpus under `ivc-file-research/samples/`
remains in iCatalog (D9).

Current local layout:

```text
iCatalog-Project/
  iCatalog/
    samples/                # Separate checkout pinned by iCatalog
  media-format-samples/      # Standalone repository; edit/reorganise here
```

The initial repository has a `main` branch and no publishing remote. The current
iCatalog checkout's local submodule URL and the submodule checkout's `origin`
point to the sibling repository. `.gitmodules` stores `../media-format-samples`:
Git resolves that relative to the parent repository's remote, not necessarily its
working directory. Once both repositories are hosted side by side under the same
GitHub owner, that relative URL can resolve there. Until then, use the local URL
override below for a fresh iCatalog checkout; ordinary recursive cloning from
GitHub cannot fetch this unpublished corpus.

## Initialise from a local sibling repository

Run from iCatalog with `media-format-samples` next to it:

```sh
git config submodule.samples.url "$(pwd)/../media-format-samples"
git -c protocol.file.allow=always submodule update --init samples
task test:corpus
```

The file-transport allowance applies to that one Git command. No global Git
security setting is changed. `git submodule sync` resets local URL overrides to
the tracked URL; restore the local override afterwards while the corpus remains
unpublished.

## Reorganise or add samples

Work in the sibling repository. Its initial files keep their supplied names;
create whatever format/application/version folders are useful. Update each
case's `source` and `reference` paths in `manifest.toml` in the same commit.
Paths are relative to the corpus root; renaming a file does not change its hash.
The manifest records provenance and licence information separately from tests.
Unknown facts are explicitly marked; iCatalog's licence does not assign a
redistribution licence to the artwork.

Commit the corpus changes, then run from iCatalog:

```sh
git -C samples fetch origin
git -C samples merge --ff-only origin/main
task test:corpus
git add samples
```

Commit the updated submodule pointer with the associated iCatalog changes.
Each iCatalog commit then selects an exact corpus commit. Changes to the sibling
repository do not automatically alter iCatalog's checked-out samples. Avoid
editing both checkouts independently unless intentionally working with branches.

## Verification

`task test:corpus` reads `samples/manifest.toml` without downloads or writes.
It checks source/export SHA-256 hashes, byte-exact extracted PNG hash, canvas and
preview dimensions, full image decoding and the allowed mean RGBA difference
against a Lanczos3-resized export. Missing corpus data, invalid paths, unknown
formats and failed expectations return an error. Supported cases are the supplied
CLIP v1 and Rebelle 8.3.4/format-8 files.
See [CLIP](clip-format.md) and [REB](reb-format.md) for sample-specific evidence.

The corpus can be absent for builds and normal tests. Publishing the sample
repository, choosing an artwork licence and adding real-corpus CI are separate
steps; none is performed by this local setup.
