"""Build every WackDepot logo file from the shared geometry in sym.py and word.py.

Needs fonttools + uharfbuzz (wordmark outlines), cairosvg (PNGs) and Pillow (favicon.ico):

    uv run --with fonttools --with uharfbuzz --with cairosvg --with pillow \
        design/logo/source/build.py
"""
import os
import re
import shutil
import subprocess
import sys

from sym import SMALL, squircle, sym
from word import outline

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "../../.."))
KIT = os.path.join(HERE, "../kit")
RENDER = os.environ.get(
    "RENDER_PNG", os.path.expanduser("~/.agents/skills/logo-design/scripts/render_png.py")
)

INK = "#111310"
LIME = "#c2ee4a"
WHITE = "#ffffff"
EDGE = "#2b3027"  # hairline lift so the dark tile holds its shape on dark Docks

# Symbol body spans y 56–200 (centre 128) and x 20–236 on its 256 canvas.
SYM_X0, SYM_X1, SYM_Y0, SYM_Y1 = 20, 236, 56, 200
GAP = 44  # symbol → wordmark: one eye height
WORD_SIZE = 110  # cap height 80; the history line runs through the middle of the caps
TRACK = -0.03
REVERSED = dict(lw=23, ew=22, eh=46, gap=14, sr=23)  # irradiation: white-on-dark reads heavier


def g(v):
    return f"{v:.2f}".rstrip("0").rstrip(".")


def doc(vb, w, h, body, title):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" width="{g(w)}" height="{g(h)}" '
        f'role="img" aria-labelledby="title"><title id="title">{title}</title>{body}</svg>\n'
    )


def write(rel, text):
    path = os.path.join(KIT, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)
    return path


def symbol(fill, **kw):
    return doc("0 0 256 256", 256, 256, f'<path id="symbol" fill="{fill}" d="{sym(**kw)}"/>', "WackDepot")


def horizontal(sym_fill, word_fill, **kw):
    x = SYM_X1 + GAP
    d, b, _, adv = outline("WackDepot", WORD_SIZE, x, 128 + 40, TRACK)
    x0, x1 = SYM_X0, x + max(adv, b[2] - x) + 4
    y0, y1 = SYM_Y0, max(SYM_Y1, b[3])
    body = f'<path id="symbol" fill="{sym_fill}" d="{sym(**kw)}"/><path id="wordmark" fill="{word_fill}" d="{d}"/>'
    return doc(f"{g(x0)} {g(y0)} {g(x1 - x0)} {g(y1 - y0)}", x1 - x0, y1 - y0, body, "WackDepot")


def stacked(sym_fill, word_fill, **kw):
    d0, b0, _, adv0 = outline("WackDepot", WORD_SIZE, 0, 0, TRACK)
    ww = adv0
    s = 1.5
    sw = (SYM_X1 - SYM_X0) * s
    W = max(ww, sw)
    sx = (W - sw) / 2 - SYM_X0 * s
    sy = -SYM_Y0 * s
    top_word = (SYM_Y1 - SYM_Y0) * s + GAP * s
    d, b, cap, _ = outline("WackDepot", WORD_SIZE, (W - ww) / 2, top_word + 80, TRACK)
    H = b[3]
    body = (
        f'<g transform="translate({g(sx)} {g(sy)}) scale({s})"><path id="symbol" fill="{sym_fill}" d="{sym(**kw)}"/></g>'
        f'<path id="wordmark" fill="{word_fill}" d="{d}"/>'
    )
    return doc(f"0 0 {g(W)} {g(H)}", W, H, body, "WackDepot")


def wordmark(fill):
    d, b, _, adv = outline("WackDepot", WORD_SIZE, 0, 0, TRACK)
    body = f'<path id="wordmark" fill="{fill}" d="{d}"/>'
    return doc(f"0 {g(b[1])} {g(adv + 4)} {g(b[3] - b[1])}", adv + 4, b[3] - b[1], body, "WackDepot")


def icon_art(small=False, margin=100, corner=True):
    """Ink tile, lime buddy, history line bleeding to the tile edges: the timeline runs past the icon.
    margin=0 + corner=False gives the full-bleed square used for web/apple-touch icons."""
    t0, t1 = margin, 1024 - margin
    tile = squircle(t0, t0, t1, t1, 240, 135) if corner else f"M{t0} {t0} H{t1} V{t1} H{t0} Z"
    p = dict(SMALL) if small else {}
    scale = 3.3 if small else 3.0
    p.setdefault("look", 8)
    ox = 512 - 128 * scale
    oy = 512 - 128 * scale
    lw = p.get("lw", 24) * scale
    bx0, _, bx1, _ = p.get("body", (52, 56, 204, 200))
    under = 16 * scale  # the line tucks under the body's edge, clear of the eyes
    left_end, right_start = ox + bx0 * scale + under, ox + bx1 * scale - under
    y = 512 - lw / 2
    face = sym(**{**p, "line": None})
    line = (
        f'<rect x="{t0}" y="{g(y)}" width="{g(left_end - t0)}" height="{g(lw)}" fill="{LIME}"/>'
        f'<rect x="{g(right_start)}" y="{g(y)}" width="{g(t1 - right_start)}" height="{g(lw)}" fill="{LIME}"/>'
    )
    edge = (
        f'<path fill="none" stroke="{EDGE}" stroke-width="4" d="{squircle(t0 + 2, t0 + 2, t1 - 2, t1 - 2, 238, 134)}"/>'
        if corner else ""
    )
    return (
        f'<path fill="{INK}" d="{tile}"/>'
        f'<clipPath id="tile"><path d="{tile}"/></clipPath><g clip-path="url(#tile)">{line}</g>'
        f'<g transform="translate({g(ox)} {g(oy)}) scale({scale})"><path fill="{LIME}" d="{face}"/></g>{edge}'
    )


def app_icon(small=False):
    return doc("0 0 1024 1024", 1024, 1024, icon_art(small), "WackDepot app icon")


def web_icon(small=False):
    return doc("0 0 1024 1024", 1024, 1024, icon_art(small, margin=0, corner=False), "WackDepot icon")


WEB_MANIFEST = """{
  "name": "WackDepot",
  "short_name": "WackDepot",
  "description": "A native macOS Git client.",
  "icons": [
    { "src": "icon-192.png", "sizes": "192x192", "type": "image/png" },
    { "src": "icon-512.png", "sizes": "512x512", "type": "image/png" },
    { "src": "maskable-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable" }
  ],
  "theme_color": "#111310",
  "background_color": "#111310",
  "display": "standalone"
}
"""


def _cairosvg():
    """cairosvg needs Homebrew's libcairo on the dyld path; re-exec once with the hint set."""
    try:
        import cairosvg
        return cairosvg
    except OSError:
        if os.environ.get("WACKDEPOT_CAIRO_HINT"):
            raise
        hints = [os.path.join(p, "lib") for p in ("/opt/homebrew", "/usr/local")]
        env = dict(os.environ)
        env["DYLD_FALLBACK_LIBRARY_PATH"] = ":".join(hints + [env.get("DYLD_FALLBACK_LIBRARY_PATH", "")])
        env["WACKDEPOT_CAIRO_HINT"] = "1"
        os.execve(sys.executable, [sys.executable] + sys.argv, env)


def render(svg, png, w, h=None, bg=None):
    os.makedirs(os.path.dirname(png), exist_ok=True)
    w, h = w, h or w
    cairosvg = _cairosvg()
    cairosvg.svg2png(url=svg, write_to=png, output_width=w, output_height=h, background_color=bg)


def main():
    if os.path.isdir(KIT):
        shutil.rmtree(KIT)
    svgs = {
        # Symbol
        "svg/wackdepot-symbol-ink.svg": symbol(INK),
        "svg/wackdepot-symbol-lime.svg": symbol(LIME, **REVERSED),
        "svg/wackdepot-symbol-black.svg": symbol("#000000"),
        "svg/wackdepot-symbol-white.svg": symbol(WHITE, **REVERSED),
        "svg/wackdepot-symbol-small-ink.svg": symbol(INK, **SMALL),
        "svg/wackdepot-symbol-small-lime.svg": symbol(LIME, **SMALL),
        # Horizontal lockup
        "svg/wackdepot-horizontal-ink.svg": horizontal(INK, INK),
        "svg/wackdepot-horizontal-reversed.svg": horizontal(LIME, WHITE, **REVERSED),
        "svg/wackdepot-horizontal-black.svg": horizontal("#000000", "#000000"),
        "svg/wackdepot-horizontal-white.svg": horizontal(WHITE, WHITE, **REVERSED),
        # Stacked lockup
        "svg/wackdepot-stacked-ink.svg": stacked(INK, INK),
        "svg/wackdepot-stacked-reversed.svg": stacked(LIME, WHITE, **REVERSED),
        "svg/wackdepot-stacked-black.svg": stacked("#000000", "#000000"),
        "svg/wackdepot-stacked-white.svg": stacked(WHITE, WHITE, **REVERSED),
        # Wordmark
        "svg/wackdepot-wordmark-ink.svg": wordmark(INK),
        "svg/wackdepot-wordmark-white.svg": wordmark(WHITE),
        # App icon masters
        "icon/wackdepot-app-icon.svg": app_icon(),
        "icon/wackdepot-app-icon-small.svg": app_icon(small=True),
        # Web icons (full-bleed square; maskable-safe: art sits inside the central 80 %)
        "web/favicon.svg": web_icon(small=True),
        "web/icon.svg": web_icon(),
    }
    paths = {k: write(k, v) for k, v in svgs.items()}

    for name in ("symbol-ink", "symbol-lime", "horizontal-ink", "horizontal-reversed", "stacked-ink", "stacked-reversed"):
        src = paths[f"svg/wackdepot-{name}.svg"]
        if name.startswith("symbol"):
            render(src, os.path.join(KIT, f"png/wackdepot-{name}-1024.png"), 1024)
            continue
        with open(src) as f:
            vb = re.search(r'viewBox="([^"]+)"', f.read()).group(1).split()
        w, h = (float(v) for v in vb[2:4])
        render(src, os.path.join(KIT, f"png/wackdepot-{name}-2400.png"), 2400, round(2400 * h / w))

    # macOS iconset: the simplified cut below 64 px, the full drawing above.
    iconset = os.path.join(ROOT, "assets/icon.iconset")
    if os.path.isdir(iconset):
        shutil.rmtree(iconset)
    for s in (16, 32, 128, 256, 512):
        for scale, suffix in ((1, ""), (2, "@2x")):
            px = s * scale
            src = paths["icon/wackdepot-app-icon-small.svg" if px < 64 else "icon/wackdepot-app-icon.svg"]
            render(src, os.path.join(iconset, f"icon_{s}x{s}{suffix}.png"), px)
    render(paths["icon/wackdepot-app-icon.svg"], os.path.join(KIT, "icon/wackdepot-app-icon-1024.png"), 1024)

    web = os.path.join(KIT, "web")
    for name, size, small in (
        ("favicon-16.png", 16, True), ("favicon-32.png", 32, True), ("favicon-48.png", 48, True),
        ("apple-touch-icon.png", 180, False), ("icon-192.png", 192, False),
        ("icon-512.png", 512, False), ("maskable-512.png", 512, False),
    ):
        render(paths["web/favicon.svg" if small else "web/icon.svg"], os.path.join(web, name), size)
    from PIL import Image
    ico = [Image.open(os.path.join(web, f"favicon-{s}.png")) for s in (16, 32, 48)]
    ico[2].save(os.path.join(web, "favicon.ico"), sizes=[(16, 16), (32, 32), (48, 48)])

    write("web/site.webmanifest", WEB_MANIFEST)
    write("web/head-snippet.html", (
        '<link rel="icon" href="/favicon.ico" sizes="48x48">\n'
        '<link rel="icon" href="/favicon.svg" type="image/svg+xml">\n'
        '<link rel="apple-touch-icon" href="/apple-touch-icon.png">\n'
        '<link rel="manifest" href="/site.webmanifest">\n'
        '<meta name="theme-color" content="#111310">\n'))
    print("built", KIT, "and", iconset)


if __name__ == "__main__":
    main()
