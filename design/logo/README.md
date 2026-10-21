# WackDepot logo

The mark is the app's mascot: a cheerful blob sitting on the history line. It reads as a
face and as a commit on a timeline at once.

- Guidelines: [`wackdepot-logo-guidelines.md`](wackdepot-logo-guidelines.md)
- Identity board with mockups: [`presentation/wackdepot-identity-board.html`](presentation/wackdepot-identity-board.html)
- First-round concepts (for the record): [`concepts-round1.png`](concepts-round1.png)

## What's here

```
source/     sym.py (geometry), word.py (wordmark outlines), build.py (builds everything)
kit/svg/    masters — symbol, horizontal, stacked, wordmark; ink / reversed / black / white
kit/png/    rasters of the colour masters
kit/icon/   macOS app icon master (1024) + the simplified small-size cut
kit/web/    favicon.svg, favicon.ico, PNG icons, apple-touch-icon, webmanifest, head snippet
presentation/  spec.json + the generated identity board
concepts/   round-1 concept SVGs and lockups
```

`assets/icon.iconset/` is generated and committed so `scripts/bundle.sh` can build the
`.icns` with no Python toolchain.

## Rebuilding

```sh
uv run --with fonttools --with uharfbuzz --with cairosvg --with pillow \
    design/logo/source/build.py
```

The wordmark is Inter Bold (bundled in `assets/fonts`, SIL OFL 1.1), shaped with HarfBuzz
and outlined to paths, so nothing downstream needs the font.

## Known limits

- Pantone for the lime is a nearest match, not a measured one.
- No trademark clearance has been done; that needs a professional search.
