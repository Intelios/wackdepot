#!/usr/bin/env python3
"""WackDepot placeholder icon — a lime git-branch mark on near-black.

Draws at 1024x1024 as raw pixels (supersampled 4x for AA), writes a PNG via
zlib (no PIL needed), then scripts/bundle.sh feeds it to iconutil.
"""

import math
import struct
import zlib

S = 4  # supersample
N = 1024 * S


def put(img, x, y, rgba):
    if 0 <= x < N and 0 <= y < N:
        img[y][x] = rgba


def circle(img, cx, cy, r, rgba):
    for y in range(int(cy - r), int(cy + r) + 1):
        for x in range(int(cx - r), int(cx + r) + 1):
            if (x - cx) ** 2 + (y - cy) ** 2 <= r * r:
                put(img, x, y, rgba)


def thick_line(img, x0, y0, x1, y1, w, rgba):
    dx, dy = x1 - x0, y1 - y0
    length = math.hypot(dx, dy)
    if length == 0:
        return
    ux, uy = dx / length, dy / length
    nx, ny = -uy, ux
    steps = int(length) + 1
    for i in range(steps):
        t = i / steps
        x, y = x0 + dx * t, y0 + dy * t
        for o in range(-w // 2, w // 2 + 1):
            put(img, int(x + nx * o), int(y + ny * o), rgba)


def curve(img, x0, y0, cx, cy, x1, y1, w, rgba, steps=400):
    for i in range(steps + 1):
        t = i / steps
        mt = 1 - t
        x = mt * mt * x0 + 2 * mt * t * cx + t * t * x1
        y = mt * mt * y0 + 2 * mt * t * cy + t * t * y1
        for o in range(-w // 2, w // 2 + 1):
            put(img, int(x), int(y + o), rgba)


def rounded(img, rad):
    for y in range(N):
        for x in range(N):
            keep = True
            if x < rad and y < rad:
                keep = (x - rad) ** 2 + (y - rad) ** 2 <= rad * rad
            elif x >= N - rad and y < rad:
                keep = (x - (N - rad - 1)) ** 2 + (y - rad) ** 2 <= rad * rad
            elif x < rad and y >= N - rad:
                keep = (x - rad) ** 2 + (y - (N - rad - 1)) ** 2 <= rad * rad
            elif x >= N - rad and y >= N - rad:
                keep = (x - (N - rad - 1)) ** 2 + (y - (N - rad - 1)) ** 2 <= rad * rad
            if not keep:
                img[y][x] = (0, 0, 0, 0)


def write_png(img, out):
    raw = bytearray()
    for by in range(0, N, S):
        raw.append(0)
        for bx in range(0, N, S):
            a = r = g = b = 0
            for dy in range(S):
                for dx in range(S):
                    pr, pg, pb, pa = img[by + dy][bx + dx]
                    r += pr * pa
                    g += pg * pa
                    b += pb * pa
                    a += pa
            if a:
                raw += bytes((r // a, g // a, b // a, a // (S * S)))
            else:
                raw += b"\x00\x00\x00\x00"

    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(
            ">I", zlib.crc32(tag + data) & 0xFFFFFFFF
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", N // S, N // S, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    open(out, "wb").write(png)


def main():
    bg = (17, 19, 16, 255)        # WackCode near-black
    accent = (194, 238, 74, 255)  # lime
    img = [[(0, 0, 0, 0) for _ in range(N)] for _ in range(N)]

    for y in range(N):
        for x in range(N):
            img[y][x] = bg
    rounded(img, int(N * 0.22))

    # Branch mark: trunk bottom → node mid → curve to node top-right.
    cx_ = int(N * 0.38)
    y_top, y_mid, y_bot = int(N * 0.28), int(N * 0.5), int(N * 0.74)
    nx = int(N * 0.62)
    lw = int(N * 0.055)

    thick_line(img, cx_, y_mid, cx_, y_bot, lw, accent)
    curve(img, cx_, y_mid, cx_, int(N * 0.34), nx, y_top, lw, accent)
    node = int(N * 0.085)
    for (x, y) in [(cx_, y_mid), (cx_, y_bot), (nx, y_top)]:
        circle(img, x, y, node, accent)
        circle(img, x, y, int(node * 0.45), bg)

    write_png(img, "/tmp/wackdepot-icon.png")
    print("wrote /tmp/wackdepot-icon.png")


if __name__ == "__main__":
    main()
