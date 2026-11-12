#!/usr/bin/env python3
"""Render DauBiko's photographic textures (faceplate, wood, knobs, switches, lamps, meter parts).

Run from the repo root:  python3 tools/render_textures.py
Writes PNG / JPEG files into plugins/daubiko/assets/ui/. Everything is procedural (numpy + Pillow),
so the look can be tuned here and re-rendered at any resolution.
"""
import math
import os

import numpy as np
from PIL import Image, ImageFilter

OUT = os.path.join(os.path.dirname(__file__), "..", "plugins", "daubiko", "assets", "ui")
os.makedirs(OUT, exist_ok=True)
RNG = np.random.default_rng(20261112)
S = 2  # texture scale relative to the 1000 x 560 GUI layout

# GUI layout (must match editor.rs): faceplate x 22..978, knob centres, big knob, screws
FACE_X0, FACE_W, FACE_H = 22, 956, 560
KNOBS = [(81, 440)] + [(170 + 66.4 * i, 440) for i in range(6)] + [(586, 440), (722, 440), (852, 440), (926, 440)]
BIG = (846, 192)


def blur(a, r):
    if r <= 0:
        return a
    from scipy.ndimage import gaussian_filter

    return gaussian_filter(a.astype(np.float32), r, mode="wrap")


def noise(h, w, r):
    return blur(RNG.standard_normal((h, w)).astype(np.float32), r)


def norm(a):
    a = a - a.min()
    return a / max(a.max(), 1e-9)


def save_rgb(a, name, q=None):
    im = Image.fromarray(np.clip(a * 255.0 + 0.5, 0, 255).astype(np.uint8), "RGB")
    path = os.path.join(OUT, name)
    if name.endswith(".jpg"):
        im.save(path, quality=q or 88, optimize=True)
    else:
        im.save(path, optimize=True)


def save_rgba(rgb, alpha, name, down=1):
    a = np.dstack([np.clip(rgb, 0, 1), np.clip(alpha, 0, 1)[..., None]])
    im = Image.fromarray((a * 255.0 + 0.5).astype(np.uint8), "RGBA")
    if down > 1:
        im = im.resize((im.width // down, im.height // down), Image.LANCZOS)
    im.save(os.path.join(OUT, name), optimize=True)


# --------------------------------------------------------------------------------------------- faceplate
def faceplate():
    H, W = FACE_H * S, FACE_W * S
    yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
    # brushed grain: per-row random level, smeared along x
    rows = RNG.standard_normal((H, 1)).astype(np.float32) * 0.6
    g = RNG.standard_normal((H, W)).astype(np.float32)
    # motion blur along x
    k = 60
    c = np.cumsum(np.pad(g, ((0, 0), (k, k)), mode="wrap"), axis=1)
    g = (c[:, 2 * k:] - c[:, :-2 * k])[:, :W] / (2 * k)
    brushed = rows + g * 3.0 + noise(H, W, 0.6) * 0.15
    brushed = (brushed - brushed.mean()) / brushed.std()

    base = np.array([0.155, 0.135, 0.165], np.float32)  # dark anodised aluminium, slightly plum
    lum = 1.0 + 0.06 * brushed
    lum *= 1.12 - 0.30 * (yy / H)  # light from above
    lum *= 1.0 - 0.10 * ((xx / W - 0.5) * 2) ** 2

    # grime: low frequency blotches, more towards the bottom and the corners
    grime = norm(noise(H, W, 60) + 0.6 * noise(H, W, 18))
    grime = np.clip(grime * 1.4 - 0.45, 0, 1) * (0.5 + 0.7 * (yy / H))
    lum *= 1.0 - 0.22 * grime

    rgb = base[None, None, :] * lum[..., None]

    # worn edges: anodising rubbed to bare metal near the border
    d = np.minimum.reduce([xx, W - 1 - xx, yy, H - 1 - yy]) / S
    wear_n = norm(noise(H, W, 3) + 0.5 * noise(H, W, 12))
    edge = np.clip((7 - d) / 7, 0, 1) ** 2
    worn = np.clip((wear_n * 0.6 + edge * 1.1 - 0.85) * 3.0, 0, 1) * 0.8
    bare = np.array([0.46, 0.44, 0.45], np.float32) * (1.0 + 0.10 * brushed)[..., None]
    rgb = rgb * (1 - worn[..., None]) + bare * worn[..., None]

    # finger polish rings around the knobs (players rub the paint there)
    def rub(cx, cy, r0, r1, amt):
        X = (cx - FACE_X0) * S
        Y = cy * S
        dist = np.sqrt((xx - X) ** 2 + (yy - Y) ** 2) / S
        ring = np.clip(1 - np.abs(dist - (r0 + r1) / 2) / ((r1 - r0) / 2), 0, 1) ** 1.2
        ring *= np.clip(norm(noise(H, W, 4)) * 1.6 - 0.2, 0, 1)
        return ring * amt

    polish = np.zeros((H, W), np.float32)
    for (kx, ky) in KNOBS:
        polish += rub(kx, ky, 22, 44, 0.30)
    polish += rub(BIG[0], BIG[1], 56, 92, 0.35)
    polish = blur(np.clip(polish, 0, 1), 3 * S)
    rgb = rgb + (bare * 0.7 - rgb) * polish[..., None] * 0.55

    # scratches
    sc = np.zeros((H, W), np.float32)
    scd = np.zeros((H, W), np.float32)
    from PIL import ImageDraw
    simg = Image.new("F", (W, H), 0.0)
    dimg = Image.new("F", (W, H), 0.0)
    sd = ImageDraw.Draw(simg)
    dd = ImageDraw.Draw(dimg)
    for _ in range(150):
        x0, y0 = RNG.uniform(0, W), RNG.uniform(0, H)
        ln = RNG.exponential(28) * S + 4 * S
        ang = RNG.uniform(0, math.pi) if RNG.random() < 0.45 else RNG.normal(0, 0.12)
        curv = RNG.normal(0, 0.0004)
        pts = []
        steps = int(ln / 6) + 2
        for i in range(steps):
            t = i / (steps - 1)
            a = ang + curv * t * ln
            pts.append((x0 + math.cos(a) * t * ln, y0 + math.sin(a) * t * ln))
        v = RNG.uniform(0.10, 0.45)
        sd.line(pts, fill=v, width=1)
        dd.line([(p[0] + 1, p[1] + 1) for p in pts], fill=v * 0.7, width=1)
    sc = np.asarray(simg, dtype=np.float32)
    scd = np.asarray(dimg, dtype=np.float32)
    rgb = rgb + sc[..., None] * 0.22 - scd[..., None] * 0.06
    # tiny dings
    for _ in range(140):
        x, y = int(RNG.uniform(0, W)), int(RNG.uniform(0, H))
        r = RNG.uniform(0.5, 1.6) * S
        y0, y1, x0, x1 = max(0, int(y - r - 2)), min(H, int(y + r + 3)), max(0, int(x - r - 2)), min(W, int(x + r + 3))
        sub = (xx[y0:y1, x0:x1] - x) ** 2 + (yy[y0:y1, x0:x1] - y) ** 2 < r * r
        rgb[y0:y1, x0:x1][sub] += 0.035
    rgb += RNG.standard_normal((H, W, 1)).astype(np.float32) * 0.006
    save_rgb(np.clip(rgb, 0, 1), "faceplate.jpg", q=90)


# --------------------------------------------------------------------------------------------- wood
def wood():
    H, W = FACE_H * S, 22 * S
    yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
    warp = noise(H, W, 30) * 40 + noise(H, W, 6) * 4
    rings = np.sin((xx * 0.9 + warp + yy * 0.02) * 0.55)
    fine = noise(H, W, 0.7) * 0.5 + blur(RNG.standard_normal((H, W)).astype(np.float32), 0) * 0.15
    streak = np.repeat(blur(RNG.standard_normal((1, W)).astype(np.float32), 1.5), H, axis=0)
    v = 0.55 + 0.18 * rings + 0.08 * fine + 0.10 * streak
    v *= 1.0 - 0.35 * (np.abs(xx / W - 0.5) * 2) ** 3  # rounded edge
    base = np.array([0.30, 0.16, 0.085], np.float32)
    rgb = base * v[..., None]
    # varnish sheen + worn corners
    rgb *= (1.05 - 0.2 * (yy / H))[..., None]
    rgb += (np.clip(0.12 - np.abs(xx / W - 0.35), 0, 1) * 0.6)[..., None] * np.array([0.25, 0.18, 0.12])
    save_rgb(np.clip(rgb, 0, 1), "wood.jpg", q=88)


# --------------------------------------------------------------------------------------------- knobs
def spun_metal(n, cx, cy, r, light_ang=-2.3, base=0.62):
    """Concentric machined aluminium: bow-tie anisotropic highlight + fine rings."""
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    dx, dy = xx - cx, yy - cy
    rr = np.sqrt(dx * dx + dy * dy)
    th = np.arctan2(dy, dx)
    rings = np.sin(rr * 2.1 + noise(n, n, 1) * 2) * 0.03 + noise(n, n, 0.5) * 0.02
    bow = (0.5 + 0.5 * np.cos(2 * (th - light_ang))) ** 6
    shade = base + 0.30 * bow + rings - 0.12 * (rr / r)
    return shade, rr


def knob_small(n=192, r_frac=0.36):
    """Static layer (shadow, bakelite skirt shading, spun aluminium cap) and rotating layer (ribs + pointer)."""
    c = n / 2
    r = n * r_frac
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    dx, dy = xx - c, yy - c
    rr = np.sqrt(dx * dx + dy * dy)
    aa = lambda d: np.clip(d, 0, 1)
    # shadow
    sx, sy = dx - 0.05 * n, dy - 0.08 * n
    sh = np.clip(1 - (np.sqrt(sx * sx + sy * sy) - r * 0.92) / (0.16 * n), 0, 1) ** 2 * 0.75
    rgb = np.zeros((n, n, 3), np.float32)
    alpha = sh.copy()
    # skirt (black bakelite with a soft top-left sheen)
    skirt = aa(r - rr + 0.5)
    lx, ly = -0.55, -0.7
    nd = (dx * lx + dy * ly) / (r + 1e-6)
    sk = 0.055 + 0.07 * np.clip(nd, 0, 1) ** 2 + 0.02 * noise(n, n, 1)
    rim = np.clip(1 - np.abs(rr - (r - 1.5)) / 1.6, 0, 1) * np.clip(nd + 0.3, 0, 1) * 0.35
    skc = (sk + rim)[..., None] * np.array([1.0, 0.96, 0.98])
    rgb = rgb * (1 - skirt[..., None]) + skc * skirt[..., None]
    alpha = np.maximum(alpha, skirt)
    # cap: spun aluminium with a bevel
    rc = r * 0.64
    cap = aa(rc - rr + 0.5)
    shade, _ = spun_metal(n, c, c, rc)
    bevel = np.clip(1 - (rc - rr) / (0.12 * rc), 0, 1) * cap
    capv = shade * (1 - 0.35 * bevel) + 0.25 * bevel * np.clip(nd, 0, 1)
    capc = capv[..., None] * np.array([0.98, 0.98, 1.0])
    rgb = rgb * (1 - cap[..., None]) + capc * cap[..., None]
    # contact shadow of the cap on the skirt
    ring = np.clip(1 - np.abs(rr - rc - 1.2) / 1.8, 0, 1) * (1 - cap)
    rgb *= (1 - 0.6 * ring)[..., None]
    save_rgba(rgb, alpha, "knob_base.png", down=1)

    # rotating layer: skirt ribs + engraved pointer (pointing up)
    rgbr = np.zeros((n, n, 3), np.float32)
    al = np.zeros((n, n), np.float32)
    th = np.arctan2(dy, dx)
    ribs = (np.cos(th * 30) > 0.55).astype(np.float32)
    zone = np.clip((rr - rc - 2) / 2, 0, 1) * np.clip((r - 1.5 - rr) / 1.5, 0, 1)
    al = np.maximum(al, ribs * zone * 0.55)
    # pointer: cream line on the skirt + engraved dark line in the cap
    px = np.abs(dx)
    up = -dy
    line_sk = aa(3.6 - px) * aa(up - rc - 1) * aa(r - 1 - up)
    line_cap = aa(2.6 - px) * aa(up - rc * 0.30) * aa(rc * 0.95 - up)
    rgbr = rgbr * (1 - line_sk[..., None]) + np.array([0.93, 0.88, 0.76]) * line_sk[..., None]
    al = np.maximum(al, line_sk)
    rgbr = rgbr * (1 - line_cap[..., None]) + np.array([0.12, 0.12, 0.13]) * line_cap[..., None]
    al = np.maximum(al, line_cap * 0.9)
    save_rgba(rgbr, al, "knob_rot.png", down=1)


def knob_big(n=320):
    c = n / 2
    r = n * 0.40
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    dx, dy = xx - c, yy - c
    rr = np.sqrt(dx * dx + dy * dy)
    th = np.arctan2(dy, dx)
    aa = lambda d: np.clip(d, 0, 1)
    lx, ly = -0.55, -0.7
    nd = (dx * lx + dy * ly) / (r + 1e-6)
    sx, sy = dx - 0.05 * n, dy - 0.09 * n
    sh = np.clip(1 - (np.sqrt(sx * sx + sy * sy) - r * 0.9) / (0.18 * n), 0, 1) ** 2 * 0.8
    rgb = np.zeros((n, n, 3), np.float32)
    alpha = sh.copy()
    body = aa(r - rr + 0.5)
    # side ring (darker anodised aluminium, plum)
    side = (0.20 + 0.25 * np.clip(nd, 0, 1) ** 1.5 + 0.03 * noise(n, n, 1))[..., None] * np.array([0.78, 0.66, 0.92])
    rgb = rgb * (1 - body[..., None]) + side * body[..., None]
    alpha = np.maximum(alpha, body)
    # top face: spun aluminium, slightly smaller, with a chamfer
    rt = r * 0.80
    top = aa(rt - rr + 0.5)
    shade, _ = spun_metal(n, c, c, rt, base=0.58)
    cham = np.clip(1 - (rt - rr) / (0.07 * rt), 0, 1) * top
    tv = shade * (1 - 0.3 * cham) + 0.35 * cham * np.clip(nd, 0, 1)
    rgb = rgb * (1 - top[..., None]) + (tv[..., None] * np.array([0.97, 0.96, 1.0])) * top[..., None]
    # shallow finger dish
    dish = np.clip(1 - rr / (rt * 0.55), 0, 1)
    rgb *= (1 - 0.10 * dish * np.clip(-nd + 0.2, 0, 1))[..., None]
    rgb += (0.08 * dish * np.clip(nd, 0, 1))[..., None]
    save_rgba(rgb, alpha, "bigknob_base.png")

    # rotating layer: diamond knurl on the side + indicator groove
    rgbr = np.zeros((n, n, 3), np.float32)
    al = np.zeros((n, n), np.float32)
    zone = np.clip((rr - rt - 2) / 2, 0, 1) * np.clip((r - 1.5 - rr) / 1.5, 0, 1)
    kn = (np.cos(th * 60 + rr * 0.9) > 0.6) | (np.cos(th * 60 - rr * 0.9) > 0.6)
    al = np.maximum(al, kn.astype(np.float32) * zone * 0.5)
    px = np.abs(dx)
    up = -dy
    groove = aa(3.0 - px) * aa(up - rt * 0.30) * aa(rt * 0.93 - up)
    rgbr = rgbr * (1 - groove[..., None]) + np.array([0.94, 0.89, 0.78]) * groove[..., None]
    al = np.maximum(al, groove)
    edge = aa(1.2 - np.abs(px - 3.4)) * aa(up - rt * 0.30) * aa(rt * 0.93 - up)
    al = np.maximum(al, edge * 0.6)
    save_rgba(rgbr, al, "bigknob_rot.png")


# --------------------------------------------------------------------------------------------- toggle, lamps, screws
def toggle(state_up):
    w, h = 96, 132
    c = (w / 2, h / 2)
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    rgb = np.zeros((h, w, 3), np.float32)
    al = np.zeros((h, w), np.float32)
    dx, dy = xx - c[0], yy - c[1]
    rr = np.sqrt(dx * dx + dy * dy)
    th = np.arctan2(dy, dx)
    aa = lambda d: np.clip(d, 0, 1)
    # shadow of the nut
    sh = np.clip(1 - (np.sqrt((dx - 4) ** 2 + (dy - 6) ** 2) - 18) / 10, 0, 1) ** 2 * 0.6
    al = np.maximum(al, sh)
    # hex nut
    hexr = 19 / np.cos(((th + math.pi / 6) % (math.pi / 3)) - math.pi / 6)
    nut = aa(hexr - rr)
    nd = (dx * -0.55 + dy * -0.7) / 19
    nv = 0.45 + 0.25 * np.clip(nd, -1, 1) + 0.04 * noise(h, w, 0.8)
    face_edge = np.clip(1 - (hexr - rr) / 3, 0, 1) * nut
    nv = nv + 0.15 * face_edge * np.sign(nd)
    rgb = rgb * (1 - nut[..., None]) + nv[..., None] * nut[..., None]
    al = np.maximum(al, nut)
    # threaded collar
    col = aa(11 - rr)
    cv = 0.35 + 0.35 * np.clip(nd, 0, 1) + 0.06 * np.sin(rr * 3)
    rgb = rgb * (1 - col[..., None]) + cv[..., None] * col[..., None]
    # lever: tapered chrome bat, pointing up or down
    s = -1 if state_up else 1
    L = 50
    t = np.clip((dy * s) / L, 0, 1)
    half = 4.0 + 3.5 * t
    inside = aa(half - np.abs(dx)) * aa(dy * s + 2) * aa(L - dy * s)
    tipc = (c[0], c[1] + s * L)
    tip = aa(9.5 - np.sqrt((xx - tipc[0]) ** 2 + (yy - tipc[1]) ** 2))
    lever = np.maximum(inside, tip)
    # chrome: horizontal gradient with sharp highlight, darker towards the root
    u = np.clip(dx / (half + 1e-3), -1, 1)
    chrome = 0.35 + 0.55 * np.exp(-((u + 0.35) ** 2) / 0.05) + 0.15 * (1 - np.abs(u)) - 0.2 * (1 - t)
    tu = (xx - tipc[0]) / 9.5
    tv = (yy - tipc[1]) / 9.5
    tipv = 0.45 + 0.5 * np.exp(-(((tu + 0.35) ** 2 + (tv + 0.4) ** 2) / 0.12)) - 0.15 * np.clip(tu + tv, 0, 2)
    lv = np.where(tip > inside, tipv, chrome)
    # lever shadow
    lsh = np.roll(np.roll(lever, 4, axis=1), 6, axis=0) * 0.5
    al = np.maximum(al, lsh)
    rgb = rgb * (1 - lsh[..., None] * (1 - lever[..., None]))
    rgb = rgb * (1 - lever[..., None]) + (lv[..., None] * np.array([0.98, 0.98, 1.0])) * lever[..., None]
    al = np.maximum(al, lever)
    save_rgba(rgb, al, "toggle_up.png" if state_up else "toggle_down.png", down=2)


def jewel(color, on, name):
    n = 96
    c = n / 2
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    dx, dy = xx - c, yy - c
    rr = np.sqrt(dx * dx + dy * dy)
    aa = lambda d: np.clip(d, 0, 1)
    col = np.array(color, np.float32)
    rgb = np.zeros((n, n, 3), np.float32)
    al = np.zeros((n, n), np.float32)
    if on:
        glow = np.clip(1 - rr / (n * 0.5), 0, 1) ** 2.2 * 0.55
        rgb += col * glow[..., None]
        al = np.maximum(al, glow)
    # chrome bezel
    bez = aa(26 - rr) * aa(rr - 18)
    nd = (dx * -0.55 + dy * -0.7) / 26
    bv = 0.4 + 0.4 * np.clip(nd, -1, 1) * np.sign(rr - 22)
    rgb = rgb * (1 - bez[..., None]) + bv[..., None] * bez[..., None]
    al = np.maximum(al, bez)
    # faceted glass dome
    dome = aa(19 - rr)
    th = np.arctan2(dy, dx)
    facet = 0.85 + 0.15 * np.cos(th * 8)
    inner = (0.55 + 0.45 * np.clip(1 - rr / 19, 0, 1)) * facet
    lvl = 1.0 if on else 0.22
    dc = col * inner[..., None] * lvl
    if on:
        dc = dc + np.clip(1 - rr / 8, 0, 1)[..., None] * 0.6
    spec = np.exp(-(((dx + 6) ** 2 + (dy + 7) ** 2) / 18)) * 0.9
    dc = dc + spec[..., None]
    rgb = rgb * (1 - dome[..., None]) + dc * dome[..., None]
    al = np.maximum(al, dome)
    save_rgba(rgb, al, name, down=2)


def screw():
    n = 64
    c = n / 2
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    dx, dy = xx - c, yy - c
    rr = np.sqrt(dx * dx + dy * dy)
    aa = lambda d: np.clip(d, 0, 1)
    sh = np.clip(1 - (np.sqrt((dx - 2) ** 2 + (dy - 3) ** 2) - 20) / 7, 0, 1) ** 2 * 0.7
    head = aa(21 - rr)
    nd = (dx * -0.55 + dy * -0.7) / 21
    hv = 0.42 + 0.3 * np.clip(nd, -1, 1) * (rr / 21) + 0.05 * noise(n, n, 0.7) - 0.15 * np.clip(rr / 21 - 0.8, 0, 1) * 5
    # slot (phillips cross), rusty grime in the slot
    slot = (aa(2.2 - np.abs(dx)) + aa(2.2 - np.abs(dy))).clip(0, 1) * aa(14 - rr)
    hv = hv * (1 - 0.75 * slot)
    rgbv = hv[..., None] * np.array([0.98, 0.96, 0.93]) + slot[..., None] * np.array([0.05, 0.03, 0.0])
    rgb = rgbv * head[..., None]
    al = np.maximum(sh, head)
    save_rgba(rgb, al, "screw.png", down=2)


# --------------------------------------------------------------------------------------------- meter parts
def vu_face():
    w, h = 200 * S, 128 * S
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    paper = np.array([0.95, 0.88, 0.70], np.float32)
    v = 1.0 + 0.025 * noise(h, w, 0.6) + 0.04 * noise(h, w, 20)
    # yellowed, darker towards the edges, warm backlight in the middle-low
    cx, cy = w / 2, h * 0.85
    d = np.sqrt(((xx - cx) / w) ** 2 + ((yy - cy) / h) ** 2)
    v *= 1.08 - 0.55 * d ** 1.6
    age = np.clip(norm(noise(h, w, 25)) * 1.3 - 0.4, 0, 1)
    rgb = paper * v[..., None]
    rgb = rgb * (1 - 0.12 * age[..., None]) + np.array([0.75, 0.55, 0.25]) * 0.12 * age[..., None]
    save_rgb(np.clip(rgb, 0, 1), "vu_face.jpg", q=90)


if __name__ == "__main__":
    faceplate()
    wood()
    knob_small()
    knob_big()
    toggle(True)
    toggle(False)
    jewel((1.0, 0.18, 0.12), True, "lamp_red_on.png")
    jewel((1.0, 0.18, 0.12), False, "lamp_red_off.png")
    jewel((1.0, 0.62, 0.15), True, "lamp_amber_on.png")
    jewel((1.0, 0.62, 0.15), False, "lamp_amber_off.png")
    screw()
    vu_face()
    for f in sorted(os.listdir(OUT)):
        print(f, os.path.getsize(os.path.join(OUT, f)))
