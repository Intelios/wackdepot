# WackDepot — Logo Guidelines (compact)

## 1. The logo

**Idea:** the mascot *is* the commit — a cheerful blob sitting on the history line.

The mark reads two ways on purpose: a face (the product's tone), and a commit on a
timeline (what the product does). The line runs to the edges of the app icon because a
history keeps going past what you can see.

**Versions:** horizontal (primary) · stacked · symbol · wordmark.

**Files:** `design/logo/kit/svg/` masters (SVG), `design/logo/kit/png/` rasters,
`design/logo/kit/icon/` app icon, `design/logo/kit/web/` favicons.
Rebuild everything with `design/logo/source/build.py`; never redraw by hand.

## 2. Construction

Built on a 256-unit grid, all parts on whole units:

| Part | Measure |
|---|---|
| Body | 152 × 144, squircle corners (64 run, 44 handles) |
| History line | 24 thick, at the body's mid-line, 16 tucked under each edge |
| Eyes | 20 × 44 pills, 16 apart, shifted 8 units forward of centre |
| Smile | half-disc, r 22, 22 below centre |
| Wordmark | Inter Bold, −0.03 em tracking, cap height 80 (≈ 0.55 × body height) |

## 3. Clear space

Leave **1 × eye height** (≈ 0.3 × the body's height) clear on every side, and at least
that much between the logo and any other element. The zone scales with the logo — never
use a fixed distance.

## 4. Minimum size

| Version | Screen | Print |
|---|---|---|
| Horizontal | 120 px wide | 30 mm wide |
| Stacked | 72 px wide | 18 mm wide |
| Symbol | 16 px (`symbol-small-*` below 64 px) | 8 mm |

Below 64 px use the small cut: heavier line, bigger eyes and smile, no fine detail.

## 5. Colour

| Name | HEX | RGB | CMYK | Pantone |
|---|---|---|---|---|
| Ink | `#111310` | 17 19 16 | 11 / 0 / 16 / 93 (rich black on press) | — |
| Lime | `#C2EE4A` | 194 238 74 | 18 / 0 / 69 / 7 | 375 C nearest — confirm on a chip |
| Paper | `#FFFFFF` | 255 255 255 | 0 / 0 / 0 / 0 | — |

Lime is an interaction colour and the mascot's body; it is **not** a text colour.

**Approved pairs**

- Ink symbol + Ink wordmark on white / paper (the default)
- Lime symbol + white wordmark on Ink `#111310` (`*-reversed`)
- One colour: black on light, white on dark (`*-black`, `*-white`)
- On photography: reversed on calm dark areas, ink on calm light areas

**Not approved:** lime on white (fails contrast), lime wordmark on a light background,
ink logo on Ink.

## 6. Typography

- Wordmark: Inter Bold v4.001, outlined to paths — no font needed to use the logo.
- App UI: Inter (Regular / Medium / SemiBold / Bold), bundled in `assets/fonts/`.
- Licence: SIL OFL 1.1; logo and app use are permitted. Keep `Inter-OFL.txt` with it.

## 7. Don'ts

Don't stretch, squash or rotate it · don't recolour the face's holes · don't move the
history line off the centre · don't redraw the face or change the eye spacing · don't add
shadows, glows, outlines or gradients · don't put the logo inside another shape · don't
set the wordmark by typing it · don't animate the smile (pose the whole body instead).

## 8. Contact

Questions: Jack. Masters and rebuild script: `design/logo/`. The app icon is generated
into `assets/icon.iconset/` and compiled to `assets/icon.icns` by `scripts/bundle.sh`.
