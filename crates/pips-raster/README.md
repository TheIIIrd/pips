# pips-raster

PNG and Aseprite pictures into the picture part of a Pips card. `no_std`
with `alloc`; the only dependencies are our own crates and `miniz_oxide`,
which `pips-format` already carries.

```text
bytes ─► decode ─► Raster (RGBA frames) ─► unscale ─► to_palette ─► Picture
```

- **PNG** — own strict decoder: every colour type, bit depth and Adam7;
  CRCs checked, chunk order enforced, nothing after IEND. Colour values as
  stored (gamma and ICC ignored: palette matches must be exact).
- **Aseprite** — every frame flattened as Aseprite shows it: visible
  layers, groups, normal blending with Aseprite's own integer formula,
  layer and cel opacity, linked cels, RGBA / grey / indexed, old and new
  palette chunks. Blend modes other than normal, tilemaps and translucent
  groups are refused: the card would not look like the file.
- **Unscaling** — an export at ×k is made of k×k blocks; k is the gcd of the
  sides and of every position where colour changes, found in one pass.
- **Palette** — exact colours: the smallest built-in palette holding them
  all, else the picture's own colours carried in the card. Or a chosen
  built-in palette with each colour mapped to the nearest (redmean, integer).
  Transparency keeps colours off index 0.

Refused with the reason, never "fixed": semi-transparent pixels (with their
place), more than 32 colours without a chosen palette, frames of unequal
duration, anything over 128×128 after unscaling. Input limits — 16 MiB,
2048 px a side, 8 Mi pixels over all frames — are checked before anything
is allocated from a header.

## Command line

```sh
pips-raster art.aseprite                       # what the conversion would do
pips-raster -p pico-8 -t "Сова" owl.png -o owl.pips && pips-card inspect owl.pips
```

## Testing

- `tests/aseprite.rs` — real Aseprite files against the PNGs Aseprite
  exported from them (from the MIT-licensed asefile project, see
  `tests/aseprite/README.md`), pixel for pixel; refusals; every truncation;
  random corruption.
- `tests/png_reference.rs` — 4000 random PNGs (all colour types, depths,
  filters, interlacing, tRNS, split IDAT) decoded by this crate and by the
  `png` crate must agree. The one deliberate difference: we refuse palette
  indices past PLTE, which the standard forbids and `png` draws.
- `tests/import.rs` — the whole pipeline: unscaling, palette choice,
  transparency, limits, a huge header refused before allocation, the
  result as a valid card.
