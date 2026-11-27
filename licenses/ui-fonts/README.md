# Embedded interface fonts

iCatalog embeds normal-width Inter for its interface and headings, and
JetBrains Mono for monospace text, following PhotoCraft's Pro desktop typography.
Both font files are unmodified and licensed under SIL OFL 1.1. The accompanying
`OFL-*.txt` files retain their copyright and licence texts.

| File in `crates/icatalog-ui/assets/fonts/` | Authors | Source revision | SHA-256 |
|---|---|---|---|
| `Inter-Variable.ttf` | The Inter Project Authors | [ArtCraft font assets](https://github.com/storytold/artcraft/tree/1e8b5b66a0ed6b01773c7509b1afd2709f4dfa97/artcraft-native/assets/fonts), `1e8b5b66a0ed6b01773c7509b1afd2709f4dfa97` | `29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031` |
| `JetBrainsMono-Regular.ttf` | The JetBrains Mono Project Authors | [PhotoCraft font assets](https://github.com/storytold/photocraft/tree/71f0d7483eb9ec56f483a2a4dbd13c2e6a0f65af/assets/fonts), `71f0d7483eb9ec56f483a2a4dbd13c2e6a0f65af` | `e6fd0d7e91550b3ed2b735d4312474362c4716edc4fc0577a0f61ed782d5aed1` |

Inter 400 supplies body/buttons and Inter 600 headings, both at optical size 14
with natural width and default tracking. Body/buttons/list metadata use 12 pt,
small text 10.5 pt, headings 15 pt and monospace 12 pt. egui's fallback stacks
and native system fallback remain enabled. No application code or brand assets
are copied. D119 supersedes the earlier Archivo/Geist Mono selection.
