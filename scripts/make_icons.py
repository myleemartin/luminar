#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成星星脱敏的应用图标（纯 Python，无第三方依赖）。

设计：深墨绿圆角方块 + 米白盾牌 + 赭红脱敏条，表达「文书保护」。
输出：32/128/128@2x/512/1024 PNG、icon.icns（macOS）、icon.ico（Windows）。

用法：python3 scripts/make_icons.py
"""

import math
import os
import struct
import zlib

BASE = 1024
SS = 2  # 超采样倍数，用于抗锯齿
W = BASE * SS

INK_DARK = (0x12, 0x24, 0x1F)
INK = (0x1B, 0x3B, 0x36)
PAPER = (0xF7, 0xF4, 0xEC)
BRONZE = (0xA3, 0x4A, 0x2A)
INK_300 = (0xA3, 0xBF, 0xB8)


def rounded_rect_cov(x, y, w, h, r):
    """返回点 (x,y) 是否在圆角矩形内（硬边，靠超采样抗锯齿）"""
    if x < 0 or y < 0 or x > w or y > h:
        return False
    cx = min(max(x, r), w - r)
    cy = min(max(y, r), h - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def shield_polygon(cx, top, half_w, bottom, shoulder):
    """构造盾牌多边形（左右对称，底部收尖）"""
    pts = [(cx, top), (cx + half_w, top + shoulder), (cx + half_w, bottom - shoulder * 1.35)]
    # 右下曲线
    for i in range(1, 25):
        t = i / 24
        # 三次贝塞尔：(右肩) -> (右下) -> (底尖)
        x = (
            (1 - t) ** 3 * (cx + half_w)
            + 3 * (1 - t) ** 2 * t * (cx + half_w * 0.92)
            + 3 * (1 - t) * t ** 2 * (cx + half_w * 0.55)
            + t ** 3 * cx
        )
        y = (
            (1 - t) ** 3 * (bottom - shoulder * 1.35)
            + 3 * (1 - t) ** 2 * t * (bottom - shoulder * 0.35)
            + 3 * (1 - t) * t ** 2 * (bottom + shoulder * 0.05)
            + t ** 3 * bottom
        )
        pts.append((x, y))
    # 镜像左侧
    left = [(2 * cx - x, y) for (x, y) in reversed(pts[1:-1])]
    pts.extend(left)
    return pts


def fill_polygon(img, w, h, poly, color, alpha=255):
    """扫描线填充多边形（偶奇规则）"""
    n = len(poly)
    ys = [p[1] for p in poly]
    y_min = max(0, int(math.floor(min(ys))))
    y_max = min(h - 1, int(math.ceil(max(ys))))
    for y in range(y_min, y_max + 1):
        yc = y + 0.5
        xs = []
        for i in range(n):
            x1, y1 = poly[i]
            x2, y2 = poly[(i + 1) % n]
            if (y1 <= yc < y2) or (y2 <= yc < y1):
                xs.append(x1 + (yc - y1) * (x2 - x1) / (y2 - y1))
        xs.sort()
        for i in range(0, len(xs) - 1, 2):
            x_start = max(0, int(math.ceil(xs[i] - 0.5)))
            x_end = min(w - 1, int(math.floor(xs[i + 1] - 0.5)))
            for x in range(x_start, x_end + 1):
                set_px(img, w, x, y, color, alpha)


def set_px(img, w, x, y, color, alpha=255):
    o = (y * w + x) * 4
    img[o] = color[0]
    img[o + 1] = color[1]
    img[o + 2] = color[2]
    img[o + 3] = alpha


def main():
    img = bytearray(W * W * 4)
    margin = 40 * SS
    radius = 210 * SS

    # 1. 背景：圆角方块 + 对角渐变
    for y in range(W):
        for x in range(W):
            if rounded_rect_cov(x - margin, y - margin, W - 2 * margin, W - 2 * margin, radius):
                t = (x + y) / (2 * W)
                r = int(INK[0] + (INK_DARK[0] - INK[0]) * t)
                g = int(INK[1] + (INK_DARK[1] - INK[1]) * t)
                b = int(INK[2] + (INK_DARK[2] - INK[2]) * t)
                set_px(img, W, x, y, (r, g, b), 255)
            else:
                set_px(img, W, x, y, (0, 0, 0), 0)

    cx = W / 2
    top = 250 * SS
    half_w = 250 * SS
    bottom = 800 * SS
    shoulder = 90 * SS
    poly = shield_polygon(cx, top, half_w, bottom, shoulder)
    fill_polygon(img, W, W, poly, PAPER)

    # 2. 盾牌内三条「文字行」，中间一条为赭红遮蔽条
    bar_h = 30 * SS
    bar_r = bar_h / 2
    rows = [
        (430 * SS, 260 * SS, INK_300),
        (515 * SS, 300 * SS, BRONZE),
        (600 * SS, 215 * SS, INK_300),
    ]
    for (by, bw, color) in rows:
        bx = cx - bw / 2
        for y in range(int(by), int(by + bar_h)):
            for x in range(int(bx), int(bx + bw)):
                if rounded_rect_cov(x - bx, y - by, bw, bar_h, bar_r):
                    set_px(img, W, x, y, color, 255)

    # 3. 下采样
    out = bytearray(BASE * BASE * 4)
    for y in range(BASE):
        for x in range(BASE):
            r = g = b = a = 0
            for dy in range(SS):
                for dx in range(SS):
                    o = ((y * SS + dy) * W + (x * SS + dx)) * 4
                    pa = img[o + 3]
                    r += img[o] * pa
                    g += img[o + 1] * pa
                    b += img[o + 2] * pa
                    a += pa
            if a == 0:
                r = g = b = 0
            else:
                r //= a
                g //= a
                b //= a
                a //= SS * SS
            o = (y * BASE + x) * 4
            out[o] = r
            out[o + 1] = g
            out[o + 2] = b
            out[o + 3] = a

    icons_dir = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "src-tauri", "icons")
    os.makedirs(icons_dir, exist_ok=True)

    # 4. 生成各尺寸 PNG
    sizes = [16, 32, 48, 64, 128, 256, 512, 1024]
    pngs = {}
    for s in sizes:
        png = resize_rgba(out, BASE, s)
        pngs[s] = encode_png(s, png)
        name = {
            32: "32x32.png",
            128: "128x128.png",
            256: "128x128@2x.png",
            512: "icon.png",
            1024: "icon_1024.png",
        }.get(s)
        if name:
            with open(os.path.join(icons_dir, name), "wb") as f:
                f.write(pngs[s])

    # 5. ICNS（macOS）
    icns_types = [
        (b"icp4", 16),
        (b"icp5", 32),
        (b"icp6", 64),
        (b"ic07", 128),
        (b"ic08", 256),
        (b"ic09", 512),
        (b"ic10", 1024),
    ]
    body = b""
    for typ, s in icns_types:
        data = pngs[s]
        body += typ + struct.pack(">I", len(data) + 8) + data
    icns = b"icns" + struct.pack(">I", len(body) + 8) + body
    with open(os.path.join(icons_dir, "icon.icns"), "wb") as f:
        f.write(icns)

    # 6. ICO（Windows），内嵌 PNG 数据
    ico_sizes = [16, 32, 48, 64, 128, 256]
    count = len(ico_sizes)
    header = struct.pack("<HHH", 0, 1, count)
    entries = b""
    offset = 6 + 16 * count
    blobs = b""
    for s in ico_sizes:
        data = pngs[s]
        b_ = 0 if s == 256 else s
        entries += struct.pack("<BBBBHHII", b_, b_, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    with open(os.path.join(icons_dir, "icon.ico"), "wb") as f:
        f.write(header + entries + blobs)

    print("图标已生成：", icons_dir)


def resize_rgba(src, src_size, dst_size):
    """简单双线性缩放"""
    if dst_size == src_size:
        return bytes(src)
    out = bytearray(dst_size * dst_size * 4)
    scale = src_size / dst_size
    for y in range(dst_size):
        sy = (y + 0.5) * scale - 0.5
        y0 = max(0, min(src_size - 1, int(math.floor(sy))))
        y1 = min(src_size - 1, y0 + 1)
        fy = sy - y0
        for x in range(dst_size):
            sx = (x + 0.5) * scale - 0.5
            x0 = max(0, min(src_size - 1, int(math.floor(sx))))
            x1 = min(src_size - 1, x0 + 1)
            fx = sx - x0
            vals = []
            for c in range(4):
                p00 = src[(y0 * src_size + x0) * 4 + c]
                p10 = src[(y0 * src_size + x1) * 4 + c]
                p01 = src[(y1 * src_size + x0) * 4 + c]
                p11 = src[(y1 * src_size + x1) * 4 + c]
                top = p00 * (1 - fx) + p10 * fx
                bot = p01 * (1 - fx) + p11 * fx
                vals.append(int(top * (1 - fy) + bot * fy + 0.5))
            o = (y * dst_size + x) * 4
            out[o : o + 4] = bytes(vals)
    return bytes(out)


def encode_png(size, rgba):
    raw = b""
    stride = size * 4
    for y in range(size):
        raw += b"\x00" + rgba[y * stride : (y + 1) * stride]

    def chunk(typ, data):
        c = struct.pack(">I", len(data)) + typ + data
        return c + struct.pack(">I", zlib.crc32(typ + data) & 0xFFFFFFFF)

    sig = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


if __name__ == "__main__":
    main()
