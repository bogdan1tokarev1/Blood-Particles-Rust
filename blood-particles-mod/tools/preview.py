#!/usr/bin/env python3
"""Draws the Blood Particles previews from the mod's own burst data.

    cargo run --example burst > burst.json
    python3 tools/preview.py burst.json

Nothing here is a screenshot of the game. The sandbox this mod was built in has
no Horizon launcher, no Minecraft 26.3 resource pack, no GPU and no display.
`examples/burst.rs` drives the mod's runtime (its counts, positions, colours
and velocities) through the particle engine's own constructor and tick
formulas; this file paints those numbers with volume, shading, highlights,
motion blur, drifting mist and the stains the landed droplets leave. The
settings page is a drawing of the strings `Config::labels()` returns, not of
the game's widgets.

Outputs (in the workspace root):
    blood-particles-preview.png     six blows at three moments, plus the dock
    blood-particles-settings.png    the page in Options > Mods, from labels()
    blood-particles-animation.gif   26 ticks of the killing blow
"""
import json
import math
import sys
from functools import lru_cache
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

GROUND = 64.0
BLOCK = 64.0                 # pixels per block (a close-up of the wound)
OUT = Path("/home/user")
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
FONT_BOLD = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
BG = (0x12, 0x13, 0x18, 255)
INK = (0xEC, 0xEE, 0xF2)
MUTED = (0x9A, 0x9E, 0xA8)
GOLD = (0xE5, 0xB5, 0x63)
PANEL = (0x1B, 0x1D, 0x24)
BUTTON = (0x28, 0x2B, 0x34)
BUTTON_EDGE = (0x46, 0x4A, 0x56)


def font(size, bold=False):
    try:
        return ImageFont.truetype(FONT_BOLD if bold else FONT, size)
    except OSError:
        return ImageFont.load_default(size=size)


def clamp(v, lo=0.0, hi=1.0):
    return max(lo, min(hi, v))


def rgb(colour, tint=1.0, lift=1.0):
    """Linear engine colour -> a display colour (a little brighter, sRGB-ish)."""
    return tuple(
        int(255 * clamp(c * lift * tint, 0, 1) ** 0.82)
        for c in colour
    )


def mix(a, b, t):
    return tuple(int(round(x + (y - x) * t)) for x, y in zip(a, b))


@lru_cache(maxsize=2048)
def droplet(radius, colour, bright, opacity):
    """A shaded, specular sphere: the game draws a textured quad, this is what
    a wet droplet reads as at this size."""
    n = max(4, radius * 2 + 3)
    y, x = np.mgrid[0:n, 0:n]
    c = (n - 1) / 2
    dx = (x - c) / radius
    dy = (y - c) / radius
    r2 = dx * dx + dy * dy
    mask = np.clip(1.0 - r2, 0.0, 1.0) ** 0.7
    normal_z = np.sqrt(np.clip(1.0 - r2, 0.0, 1.0))
    # One light, high and to the left; the underside of a drop is nearly black.
    lambert = 0.32 + 0.68 * np.clip(-dx * 0.55 - dy * 0.7 + normal_z * 0.62, 0.0, 1.0)
    spec = np.exp(-((dx + 0.34) ** 2 + (dy + 0.42) ** 2) / 0.035)
    rim = np.clip((r2 - 0.55) / 0.45, 0.0, 1.0) * 0.18
    shine = spec * (0.85 if bright else 0.55) + rim
    arr = np.zeros((n, n, 4), dtype=np.float32)
    for i in range(3):
        arr[..., i] = (colour[i] / 255.0) * lambert * 255.0 + shine * (255.0 - colour[i] * 0.55)
    arr[..., 3] = mask * 255.0 * opacity
    return Image.fromarray(np.clip(arr, 0, 255).astype(np.uint8), "RGBA")


@lru_cache(maxsize=2048)
def puff(radius, colour, opacity):
    """A soft blob for mist and clouds: no hard edge, no highlight."""
    n = max(6, radius * 2 + 3)
    y, x = np.mgrid[0:n, 0:n]
    c = (n - 1) / 2
    r2 = ((x - c) / radius) ** 2 + ((y - c) / radius) ** 2
    alpha = np.exp(-r2 * 2.1) * opacity
    arr = np.zeros((n, n, 4), dtype=np.float32)
    arr[..., 0] = colour[0]
    arr[..., 1] = colour[1]
    arr[..., 2] = colour[2]
    arr[..., 3] = np.clip(alpha * 255.0, 0, 255)
    return Image.fromarray(arr.astype(np.uint8), "RGBA")


def project(p, ox, oy):
    """A slight oblique view: z moves a drop a little right and up."""
    return (
        ox + (p[0] + 0.24 * p[2] + 2.5) * BLOCK,
        oy + (GROUND + 2.55 - p[1] - 0.13 * p[2]) * BLOCK,
    )


def paste(base, sprite, x, y):
    ix, iy = int(round(x - sprite.width / 2)), int(round(y - sprite.height / 2))
    if ix < 0 or iy < 0 or ix + sprite.width > base.width or iy + sprite.height > base.height:
        # Paste clipped: alpha_composite refuses an out-of-bounds destination.
        box = (max(0, -ix), max(0, -iy),
               min(sprite.width, base.width - ix), min(sprite.height, base.height - iy))
        if box[2] <= box[0] or box[3] <= box[1]:
            return
        sprite = sprite.crop(box)
        ix += box[0]
        iy += box[1]
    base.alpha_composite(sprite, (ix, iy))


def background(img, draw, ox, oy, w, h):
    """Ground, a little depth and a blocky mob silhouette with a rim light."""
    # A dark, slightly graded sky.
    for row in range(h):
        t = row / h
        shade = int(0x14 + 10 * t)
        draw.line([(ox, oy + row), (ox + w, oy + row)], fill=(shade, shade + 1, shade + 5, 255))
    gy = project([0.0, GROUND, 0.0], ox, oy)[1]
    draw.rectangle([ox, gy, ox + w, oy + h], fill=(0x2B, 0x2C, 0x2E, 255))
    draw.line([(ox, gy), (ox + w, gy)], fill=(0x55, 0x57, 0x59, 255), width=2)
    for i in range(1, 7):
        xx = ox + i * w / 7
        draw.line([(xx, gy + 4), (xx - 12, oy + h)], fill=(0x33, 0x34, 0x37, 255), width=1)
    # The mob: body, head, two arms, its wound in the upper body.
    cx = project([0.0, GROUND, 0.0], ox, oy)[0]
    top = project([0.0, GROUND + 1.95, 0.0], ox, oy)[1]
    bw, hh = 0.28 * BLOCK, 0.32 * BLOCK
    arm = 0.11 * BLOCK
    body = (cx - bw, top + hh, cx + bw, gy - 1)
    head = (cx - bw * 0.8, top, cx + bw * 0.8, top + hh)
    draw.rectangle(head, fill=(0x3B, 0x3D, 0x42, 255), outline=(0x6C, 0x70, 0x79, 255))
    draw.rectangle(body, fill=(0x35, 0x37, 0x3D, 255), outline=(0x64, 0x69, 0x72, 255))
    draw.rectangle((cx - bw - arm, top + hh + 2, cx - bw, gy - 0.18 * BLOCK),
                   fill=(0x33, 0x35, 0x3B, 255))
    draw.rectangle((cx + bw, top + hh + 2, cx + bw + arm, gy - 0.18 * BLOCK),
                   fill=(0x33, 0x35, 0x3B, 255))
    draw.line([(body[0], body[1]), (body[0], body[3])], fill=(0x77, 0x7C, 0x86, 255))
    wx, wy = project([0.0, GROUND + 1.95 * 0.62, 0.0], ox, oy)
    draw.ellipse([wx - 3, wy - 3, wx + 3, wy + 3], fill=(0x8A, 0x24, 0x24, 255))
    return gy


def draw_frame(base, scene, tick, ox, oy, w, h, show_caption=True):
    draw = ImageDraw.Draw(base)
    gy = background(base, draw, ox, oy, w, h)
    # Stains first: dark, flat, on the ground, growing as drops come to rest.
    for drop in scene["drops"]:
        if drop["kind"] not in ("droplet", "highlight"):
            continue
        end = min(int(drop["life"]), len(drop["path"]) - 1)
        position = drop["path"][end]
        if position[1] > GROUND + 0.03 or end > tick:
            continue
        sx, sy = project([position[0], GROUND, position[2]], ox, oy)
        dark = rgb(scene["colours"]["mist_to"], lift=1.4)
        r = max(2, int(1.5 * BLOCK * drop["size"]))
        draw.ellipse([sx - r, sy - r * 0.45, sx + r, sy + r * 0.45],
                     fill=(*dark, 210))
        draw.ellipse([sx - r * 0.4, sy - r * 0.3, sx + r * 0.1, sy - r * 0.1],
                     fill=(*mix(dark, (255, 210, 210), 0.18), 90))
    # Mist and clouds under the droplets: soft, translucent, drying darker.
    for drop in scene["drops"]:
        if drop["kind"] not in ("mist", "cloud"):
            continue
        path = drop["path"]
        at = min(tick, len(path) - 1)
        if tick > drop["life"]:
            continue
        p = path[at]
        px, py = project(p, ox, oy)
        age = clamp(at / drop["life"])
        colour = mix(rgb(drop["from"], drop["tint"]), rgb(drop["to"]), age)
        grow = 0.65 + 0.75 * age
        radius = max(3, int(BLOCK * drop["size"] * (2.9 if drop["kind"] == "cloud" else 2.1) * grow))
        fade = (1.0 - age ** 2) * (0.32 if drop["kind"] == "cloud" else 0.26)
        paste(base, puff(radius, colour, fade), px, py)
    # Droplets: motion blur behind, then the shaded sphere with its highlight.
    for drop in scene["drops"]:
        if drop["kind"] not in ("droplet", "highlight"):
            continue
        path = drop["path"]
        at = min(tick, len(path) - 1)
        if tick > drop["life"]:
            continue
        age = clamp(at / drop["life"])
        colour = mix(rgb(drop["from"], drop["tint"]), rgb(drop["to"]), age * 0.65)
        bright = drop["kind"] == "highlight"
        radius = max(2, int(BLOCK * drop["size"] * (1.25 if bright else 1.1)))
        fade = clamp((1.0 - age) / 0.32)
        for back in (3, 2, 1):
            older = at - back
            if older < 0:
                continue
            gx, gy2 = project(path[older], ox, oy)
            ghost = droplet(max(2, radius - back // 2), colour, bright,
                            0.17 * fade * (1 - back / 4))
            paste(base, ghost, gx, gy2)
        px, py = project(path[at], ox, oy)
        paste(base, droplet(radius, colour, bright, 0.96 * fade), px, py)
    # A badge that says what this picture is, in every frame.
    draw = ImageDraw.Draw(base)
    draw.rounded_rectangle([ox + 8, oy + 8, ox + 116, oy + 26], radius=4, fill=(0x0B, 0x0C, 0x10, 220))
    draw.text((ox + 14, oy + 11), "SIMULATION", fill=GOLD, font=font(10, bold=True))
    if show_caption:
        draw.rectangle([ox, oy + h - 52, ox + w, oy + h], fill=(0x0E, 0x0F, 0x14, 235))
        draw.text((ox + 10, oy + h - 49), scene["title"], fill=INK, font=font(13, bold=True))
        drops = sum(d["kind"] in ("droplet", "highlight") for d in scene["drops"])
        mist = sum(d["kind"] in ("mist", "cloud") for d in scene["drops"])
        caption = scene.get("caption", "")
        while caption and draw.textlength(caption + "...", font=font(10)) > w - 20:
            caption = caption[:-1]
        if caption != scene.get("caption", ""):
            caption += "..."
        draw.text((ox + 10, oy + h - 30), caption, fill=(0xB9, 0xBE, 0xC8), font=font(10))
        line = f"{scene['blood']}  |  {drops} droplets + {mist} mist/clouds  |  tick {tick}"
        draw.text((ox + 10, oy + h - 16), line, fill=MUTED, font=font(10))
    return gy


def sheet(data, path):
    w, h = 320, 226
    cols, ticks = 3, (3, 9, 17)
    rows = len(data["scenes"])
    header, dock = 104, 300
    width = 40 + cols * w
    height = header + rows * h + dock
    img = Image.new("RGBA", (width, height), BG)
    draw = ImageDraw.Draw(img)
    draw.text((20, 14), f"Blood Particles {data['version']}", fill=INK, font=font(24, bold=True))
    draw.text((20, 46), "Six blows, three moments each - drawn from the mod's own burst data",
              fill=MUTED, font=font(12))
    draw.text((20, 68), "Simulated from runtime.rs -> examples/burst.rs, not a screenshot of the game.",
              fill=GOLD, font=font(11))
    draw.line([(20, 92), (width - 20, 92)], fill=(0x3A, 0x3D, 0x46), width=1)
    for row, scene in enumerate(data["scenes"]):
        for col, tick in enumerate(ticks):
            scene = dict(scene, version=data["version"])
            ox, oy = 20 + col * w, header + row * h
            draw_frame(img, scene, tick, ox, oy, w, h)
            ImageDraw.Draw(img).rectangle([ox, oy, ox + w, oy + h], outline=(0x39, 0x3C, 0x44))
    draw_settings_dock(img, data, 20, header + rows * h + 12, width - 40, dock - 24)
    img.convert("RGB").save(path, optimize=True)
    print("wrote", path, img.size)


def draw_settings_dock(img, data, ox, oy, w, h):
    """The mod's page in Options > Mods, as a dock under the sheet: the exact
    strings Config::labels() gives the game, in the order a click uses."""
    draw = ImageDraw.Draw(img)
    draw.rounded_rectangle([ox, oy, ox + w, oy + h], radius=8, fill=PANEL, outline=(0x3D, 0x41, 0x4B))
    draw.text((ox + 14, oy + 10), "Options > Mods > Blood Particles", fill=INK, font=font(14, bold=True))
    draw.text((ox + 14, oy + 32), "One button per setting. Clicking it moves the value on; the game asks for "
              "the labels again. The last button resets everything.", fill=MUTED, font=font(10))
    labels = data["settings"]
    rows = math.ceil(len(labels) / 2)
    bw = (w - 46) / 2
    bh = 21
    for i, label in enumerate(labels):
        col, row = i % 2, i // 2
        bx = ox + 14 + col * (bw + 18)
        by = oy + 50 + row * (bh + 4)
        last = label == "Reset to defaults"
        draw.rounded_rectangle([bx, by, bx + bw, by + bh], radius=3,
                               fill=(0x3A, 0x25, 0x27) if last else BUTTON,
                               outline=(0x8C, 0x4A, 0x45) if last else BUTTON_EDGE)
        draw.text((bx + 8, by + (bh - 12) / 2), label, fill=(0xF0, 0xC9, 0xC9) if last else INK,
                  font=font(10))
        if not last and i < len(data.get("keys", [])):
            key = data["keys"][i]
            draw.text((bx + bw - 10 - draw.textlength(key, font=font(9)), by + (bh - 11) / 2),
                      key, fill=(0x68, 0x6D, 0x78), font=font(9))
    draw.text((ox + 14, oy + h - 20),
              f"Labels: Config::labels()  |  /bloodparticles <setting> <value>  |  {data['config'][:64]}...",
              fill=(0x70, 0x74, 0x7E), font=font(9))


def settings_page(data, path):
    w, h = 860, 690
    img = Image.new("RGBA", (w, h), BG)
    draw = ImageDraw.Draw(img)
    draw.text((34, 22), "Options > Mods", fill=MUTED, font=font(13))
    draw.text((34, 44), f"Blood Particles {data['version']}", fill=INK, font=font(25, bold=True))
    draw.text((34, 80), "A hurt mob sprays realistic blood. Every part of it is configurable.",
              fill=MUTED, font=font(12))
    draw.line([(34, 108), (w - 34, 108)], fill=(0x3A, 0x3D, 0x46))
    # The mod's row, as the game lists it.
    draw.rounded_rectangle([34, 122, w - 34, 170], radius=5, fill=PANEL, outline=BUTTON_EDGE)
    draw.text((48, 132), f"Blood Particles {data['version']}", fill=INK, font=font(14, bold=True))
    draw.text((48, 152), "On", fill=(0x7D, 0xD0, 0x8B), font=font(11))
    draw.text((w - 96, 138), "ENABLED", fill=(0x7D, 0xD0, 0x8B), font=font(10, bold=True))
    draw.text((34, 186), "Settings", fill=INK, font=font(15, bold=True))
    labels = data["settings"]
    bw = w - 68
    for i, label in enumerate(labels):
        bx, by = 34, 214 + i * 29
        last = label == "Reset to defaults"
        draw.rounded_rectangle([bx, by, bx + bw, by + 25], radius=3,
                               fill=(0x3A, 0x25, 0x27) if last else BUTTON,
                               outline=(0x8C, 0x4A, 0x45) if last else BUTTON_EDGE)
        draw.text((bx + 12, by + 6), label, fill=(0xF0, 0xC9, 0xC9) if last else INK, font=font(11))
        if not last and i < len(data.get("keys", [])):
            key = data["keys"][i]
            draw.text((bx + bw - 12 - draw.textlength(key, font=font(10)), by + 7),
                      key, fill=(0x6D, 0x72, 0x7C), font=font(10))
    draw.text((34, h - 76), "Drawn from Config::labels() and KEYS; the game draws the widgets. "
              "This is a simulation, not a screenshot.", fill=GOLD, font=font(10))
    draw.text((34, h - 56), "The same 12 settings answer to /bloodparticles <setting> <value>, or reset.",
              fill=MUTED, font=font(10))
    draw.text((34, h - 34), f"Saved line: {data['config']}", fill=(0x74, 0x78, 0x82), font=font(9))
    img.convert("RGB").save(path, optimize=True)
    print("wrote", path, img.size)


def animation(data, path):
    scene = next(s for s in data["scenes"] if s["death"])
    w, h = 420, 300
    frames = []
    for tick in range(0, 26):
        img = Image.new("RGBA", (w, h), BG)
        draw_frame(img, dict(scene, version=data["version"]), tick, 0, 0, w, h, show_caption=False)
        draw = ImageDraw.Draw(img)
        draw.rectangle([0, h - 44, w, h], fill=(0x0E, 0x0F, 0x14, 240))
        draw.text((12, h - 40), "The killing blow - three times as much of everything",
                  fill=INK, font=font(13, bold=True))
        draw.text((12, h - 21), f"tick {tick} / 25   |   simulated, not a screenshot",
                  fill=MUTED, font=font(10))
        frames.append(img.convert("RGB"))
    frames[0].save(path, save_all=True, append_images=frames[1:], duration=90,
                   loop=0, optimize=True)
    print("wrote", path, frames[0].size, len(frames), "frames")


if __name__ == "__main__":
    source = sys.argv[1] if len(sys.argv) > 1 else "burst.json"
    data = json.load(open(source))
    sheet(data, OUT / "blood-particles-preview.png")
    settings_page(data, OUT / "blood-particles-settings.png")
    animation(data, OUT / "blood-particles-animation.gif")
