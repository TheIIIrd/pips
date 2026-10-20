# pips-palette

Palettes for Pips cards: a built-in set referenced by one-byte ids, strict
import of Lospec `.hex` and GIMP/Aseprite `.gpl` files, and export to both.
`no_std` + `alloc`, no dependencies, no `unsafe`.

A palette has 2 to 32 colours. Index 0 is the background in every palette.

```rust
let p = pips_palette::builtin_by_slug("endesga-32").unwrap().palette();
let imported = pips_palette::parse(std::fs::read("mine.gpl")?.as_slice())?;
println!("{}", pips_palette::to_hex(&imported.palette));
```

## Built-ins

| id | slug | colours | |
|---|---|---|---|
| 0 | `pips-16` | 16 | the default |
| 1 | `pips-32` | 32 | Pips 16 plus 16; its first 16 colours *are* Pips 16 |
| 2–11 | `night` `bloom` `rust` `ink` `deep` `arcade` `castle` `neon` `hallow` `frost` | 16 | themed |
| 16 | `pico-8` | 16 | Lexaloffle Games |
| 17 | `sweetie-16` | 16 | GrafxKid |
| 18 | `dawnbringer-16` | 16 | DawnBringer |
| 19 | `dawnbringer-32` | 32 | DawnBringer |
| 20 | `endesga-32` | 32 | ENDESGA |
| 21 | `game-boy` | 4 | |
| 22 | `cga` | 16 | |
| 23 | `commodore-64` | 16 | |
| 24 | `zughy-32` | 32 | Zughy |
| 25 | `jehkoba-32` | 32 | Jehkoba |
| 26 | `afr-32` | 32 | Alpha6 |
| 27 | `downgraded-32` | 32 | downgraded |
| 28 | `aerugo-32` | 32 | Lizalot |
| 29 | `antique-tarot-32` | 32 | SurrealEmber |
| 30 | `hilda-32` | 32 | skeddles |
| 31 | `nanner-pancakes-32` | 32 | Afterimage |
| 32 | `pastel-horizon-32` | 32 | BurakoIRL |
| 33 | `softmilk-32` | 32 | MortMort |
| 34 | `lost-century-16` | 16 | SurrealEmber |
| 35 | `steam-lords-16` | 16 | Slynyrd |
| 36 | `galaxy-flame-16` | 16 | Rhoq |

Ids 0–15 are for palettes designed for Pips (12–15 free), 16 and up for
everything else (37–63 free). Ids above 63 are reserved. **Nothing is frozen
yet.** Once format v1 is frozen, an id never changes meaning and a palette
never changes colour.

Every palette with a file in `tests/lospec/` is checked against it on every
test run; those files are the `.hex` exports published on Lospec.

## Command line

```sh
pips-palette list                        # id, slug, count, name, credit
pips-palette show -f gpl pico-8 > p.gpl  # a built-in, as .hex (default) or .gpl
pips-palette convert p.gpl               # check any .hex/.gpl, print normalised
```

Exit status: 0 success, 1 bad input or I/O error, 2 usage error.

## Parsing rules

Input over 64 KiB is refused unread; parsing stops at the 33rd colour.
`.hex` takes six hex digits per line with optional `#`. `.gpl` takes the
`GIMP Palette` header, `Name:`, `Columns:`, `Channels: RGB`, `#` comments and
`R G B [name]` lines of one to three decimal digits. Errors name the line.
