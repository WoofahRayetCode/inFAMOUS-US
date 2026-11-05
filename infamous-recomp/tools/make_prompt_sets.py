"""Button-prompt glyph sets for other controllers, drawn in the game's own icon style from the player's dump:
  xbox      A / B / X / Y in their colours, LB / RB / LT / RT, LS / RS, Menu / View
  nintendo  B / A / Y / X (Switch positions), L / R / ZL / ZR, LS / RS, + / -
  steam     A / B / X / Y (Steam Deck), the game's L1 / R1 / L2 / R2 and L3 / R3, Menu / View
PlayStation prompts stay the game's originals; the keyboard/mouse set is make_kbm_icons.py + make_kbm_prompts.py.
For each set this writes, in the containers rsx_d3d12_backend.c reads (one entry per replaced texture):
  assets/<set>_icons.bin    "IFKBMIC1": the 512x256 icon-font atlas (font_icons_00.dds level 0): original, then the set's
  assets/<set>_prompts.bin  "IFKBMIC2": the 11 HUD button-prompt textures of core.xpp: original, then the set's
Inputs: assets/font_icons_l0.bin (tools/dump_icon_tex.py) and core.xpp (tools/psarc.py ... extract core.xpp).
Needs numpy + Pillow + a bold TrueType font (KBM_FONT, else DejaVu Sans Bold or Arial Bold).
PROMPT_PREVIEW_DIR=<dir> also writes <set>_atlas.png and <set>_prompts.png to look at.
usage (from infamous-recomp/tools): python make_prompt_sets.py <core.xpp> [xbox|nintendo|steam ...]   (default: all)"""
import os, sys, struct, numpy as np
from PIL import Image, ImageDraw
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from kbm_glyphs_dxt import dxt5_decode, dxt5_encode, tex_csum
from kbm_glyphs_draw import CELLS, fit_text
import make_kbm_prompts as HUD
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
S = 4
WHITE = (255, 255, 255, 255)
XBOX = {'A': (96, 200, 72, 255), 'B': (236, 64, 64, 255), 'X': (72, 150, 236, 255), 'Y': (250, 196, 40, 255)}
# Icon-font cells (kbm_glyphs_draw.CELLS): F080 Circle, F081 Square, F082 Triangle, F083 Cross, F084 L1, F085 L2, F086 R1,
# F087 R2, F088 START, F089 SELECT, F08A left stick, F08B L3, F08C right stick, F08D R3, F090-F094 d-pad (kept as drawn).
# HUD textures (make_kbm_prompts.OIDS order): icon triangle/circle/square/x, symbol triangle/circle/square/x, L1/R1/L2.
SETS = {
    'xbox': dict(face={0xF083: ('A', XBOX['A']), 0xF080: ('B', XBOX['B']), 0xF081: ('X', XBOX['X']), 0xF082: ('Y', XBOX['Y'])},
                 label={0xF084: 'LB', 0xF085: 'LT', 0xF086: 'RB', 0xF087: 'RT'}, stick={0xF08B: 'LS', 0xF08D: 'RS'},
                 system={0xF088: ('menu', 'MENU'), 0xF089: ('view', 'VIEW')},
                 hud_face=[('Y', XBOX['Y']), ('B', XBOX['B']), ('X', XBOX['X']), ('A', XBOX['A'])], hud_label=['LB', 'RB', 'LT']),
    'nintendo': dict(face={0xF083: ('B', WHITE), 0xF080: ('A', WHITE), 0xF081: ('Y', WHITE), 0xF082: ('X', WHITE)},
                     label={0xF084: 'L', 0xF085: 'ZL', 0xF086: 'R', 0xF087: 'ZR'}, stick={0xF08B: 'LS', 0xF08D: 'RS'},
                     system={0xF088: ('plus', ''), 0xF089: ('minus', '')},
                     hud_face=[('X', WHITE), ('A', WHITE), ('Y', WHITE), ('B', WHITE)], hud_label=['L', 'R', 'ZL']),
    'steam': dict(face={0xF083: ('A', WHITE), 0xF080: ('B', WHITE), 0xF081: ('X', WHITE), 0xF082: ('Y', WHITE)},
                  label={}, stick={}, system={0xF088: ('menu', 'MENU'), 0xF089: ('view', 'VIEW')},
                  hud_face=[('Y', WHITE), ('B', WHITE), ('X', WHITE), ('A', WHITE)], hud_label=['L1', 'R1', 'L2']),
}
def to_pil(a): return Image.fromarray(np.ascontiguousarray(a).astype(np.uint8), 'RGBA')
def dark_face(cell):
    """median colour of the dark, unsaturated, opaque pixels = the button face"""
    p = cell.reshape(-1, 4).astype(float); sat = p[:, :3].max(1) - p[:, :3].min(1)
    dark = p[(p[:, 3] > 200) & (sat < 30) & (p[:, :3].mean(1) < 70), :3]
    return tuple(int(v) for v in np.median(dark, 0)) + (255,) if len(dark) else (30, 34, 42, 255)
def opaque_box(cell, a=128):
    ys, xs = np.nonzero(cell[..., 3] > a); return xs.min(), ys.min(), xs.max() + 1, ys.max() + 1
def text_in(dr, box, txt, col, frac=0.66, stroke=0):
    """centred text, the largest size that fits the box"""
    x0, y0, x1, y1 = box; f, b = fit_text(dr, txt, (x1 - x0) * 0.86, (y1 - y0) * frac)
    tw, th = b[2] - b[0], b[3] - b[1]
    dr.text((x0 + (x1 - x0 - tw) / 2 - b[0], y0 + (y1 - y0 - th) / 2 - b[1]), txt, font=f, fill=col,
            stroke_width=stroke, stroke_fill=(0, 0, 0, 255))
def face_button(cell, letter, col):
    """dark round button with a coloured symbol -> the same button with a letter"""
    H, W = cell.shape[:2]; face = dark_face(cell)
    big = to_pil(cell).resize((W * S, H * S), Image.LANCZOS); dr = ImageDraw.Draw(big)
    x0, y0, x1, y1 = opaque_box(cell); cx, cy = (x0 + x1) / 2 * S, (y0 + y1) / 2 * S
    r = min(x1 - x0, y1 - y0) / 2 * S * 0.80
    dr.ellipse([cx - r, cy - r, cx + r, cy + r], fill=face)
    text_in(dr, (cx - r, cy - r, cx + r, cy + r), letter, col, 0.74)
    return np.array(big.resize((W, H), Image.LANCZOS))
def label_button(cell, txt):
    """bevelled dark label (L1 ...) -> the same label with other text"""
    H, W = cell.shape[:2]; face = dark_face(cell)
    big = to_pil(cell).resize((W * S, H * S), Image.LANCZOS); dr = ImageDraw.Draw(big)
    x0, y0, x1, y1 = opaque_box(cell, 200); m = 4 * S
    dr.rounded_rectangle([x0 * S + m, y0 * S + m, x1 * S - m, y1 * S - m], radius=3 * S, fill=face)
    text_in(dr, (x0 * S, y0 * S, x1 * S, y1 * S), txt, WHITE, 0.62, stroke=int(1.2 * S))
    return np.array(big.resize((W, H), Image.LANCZOS))
def stick_button(atlas, code, plain, txt):
    """L3 / R3: the plain stick picture with LS / RS on it"""
    x, y, w, h = CELLS[code]; px, py, pw, ph = CELLS[plain]
    cell = atlas[py:py + ph, px:px + pw]
    if (pw, ph) != (w, h): cell = np.array(to_pil(cell).resize((w, h), Image.LANCZOS))
    big = to_pil(cell).resize((w * S, h * S), Image.LANCZOS); dr = ImageDraw.Draw(big)
    text_in(dr, (0, 0, w * S, h * S * 0.70), txt, WHITE, 0.60, stroke=int(1.5 * S))
    return np.array(big.resize((w, h), Image.LANCZOS))
def system_button(w, h, kind, caption):
    """START / SELECT -> Menu / View (Xbox, Steam Deck) or + / - (Nintendo): a dark round button with the icon, caption above"""
    big = Image.new('RGBA', (w * S, h * S), (0, 0, 0, 0)); dr = ImageDraw.Draw(big)
    face, rim = (30, 34, 42, 255), (160, 166, 176, 255)
    top = 0
    if caption:
        text_in(dr, (0, 0, w * S, h * S * 0.40), caption, WHITE, 0.80, stroke=int(1.2 * S)); top = h * S * 0.40
    r = min(w * S, h * S - top) * 0.40; cx, cy = w * S / 2, top + (h * S - top) / 2
    dr.ellipse([cx - r, cy - r, cx + r, cy + r], fill=face, outline=rim, width=int(1.5 * S))
    g = r * 0.55; t = int(2.2 * S)
    if kind == 'menu':
        for k in (-1, 0, 1): dr.line([cx - g, cy + k * g * 0.75, cx + g, cy + k * g * 0.75], fill=WHITE, width=t)
    elif kind == 'view':
        o = g * 0.5
        dr.rectangle([cx - g, cy - g + o, cx + g - o, cy + g], outline=WHITE, width=t)
        dr.rectangle([cx - g + o, cy - g, cx + g, cy + g - o], fill=face, outline=WHITE, width=t)
    else:
        dr.line([cx - g, cy, cx + g, cy], fill=WHITE, width=t)
        if kind == 'plus': dr.line([cx, cy - g, cx, cy + g], fill=WHITE, width=t)
    return np.array(big.resize((w, h), Image.LANCZOS))
def build_atlas(orig_raw, spec):
    vis = dxt5_decode(orig_raw, 512, 256)[::-1]; out = vis.copy()
    def cell(code): x, y, w, h = CELLS[code]; return vis[y:y + h, x:x + w].copy()
    def put(code, im): x, y, w, h = CELLS[code]; out[y:y + h, x:x + w] = im
    for code, (letter, col) in spec['face'].items(): put(code, face_button(cell(code), letter, col))
    for code, txt in spec['label'].items(): put(code, label_button(cell(code), txt))
    for code, txt in spec['stick'].items(): put(code, stick_button(vis, code, 0xF08A if code == 0xF08B else 0xF08C, txt))
    for code, (kind, caption) in spec['system'].items(): put(code, system_button(CELLS[code][2], CELLS[code][3], kind, caption))
    return out
def write_icons(orig_raw, atlas, path):
    enc = dxt5_encode(np.ascontiguousarray(atlas[::-1]))                  # GPU textures are stored bottom-up
    open(path, 'wb').write(b'IFKBMIC1' + struct.pack('<5I', 512, 256, 0x88, 131072, tex_csum(orig_raw, 131072)) + orig_raw + enc)
    print('wrote', path)
def hud_jobs(spec, oids):
    jobs = [(oid, HUD.round_button, letter, col) for oid, (letter, col) in zip(oids[0:4], spec['hud_face'])]
    jobs += [(oid, HUD.symbol, letter, col) for oid, (letter, col) in zip(oids[4:8], spec['hud_face'])]
    jobs += [(oid, HUD.label, txt, None) for oid, txt in zip(oids[8:11], spec['hud_label'])]
    return jobs
def main():
    if len(sys.argv) < 2: raise SystemExit(__doc__)
    sets = sys.argv[2:] or list(SETS)
    src = os.path.join(ROOT, 'assets', 'font_icons_l0.bin')
    if not os.path.isfile(src): raise SystemExit('make_prompt_sets: assets/font_icons_l0.bin not found (run tools/dump_icon_tex.py first)')
    orig_raw = open(src, 'rb').read()[:131072]
    HUD.load_xpp(sys.argv[1]); R = HUD.region(); print('release', R)
    preview = os.environ.get('PROMPT_PREVIEW_DIR')
    for name in sets:
        spec = SETS[name]
        atlas = build_atlas(orig_raw, spec)
        write_icons(orig_raw, atlas, os.path.join(ROOT, 'assets', name + '_icons.bin'))
        HUD.build(hud_jobs(spec, HUD.OIDS[R]), os.path.join(ROOT, 'assets', name + '_prompts.bin'),
                  os.path.join(preview, name + '_prompts.png') if preview else None)
        if preview:
            bg = np.full(atlas.shape[:2] + (3,), 60.0); al = atlas[..., 3:4] / 255.0
            Image.fromarray((atlas[..., :3] * al + bg * (1 - al)).astype(np.uint8)).save(os.path.join(preview, name + '_atlas.png'))
if __name__ == '__main__':
    main()
