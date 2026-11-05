"""v197: build assets/kbm_prompts.bin -- keyboard/mouse versions of the HUD button-prompt textures in core.xpp
(context prompts like "Collect Dead Drop": the diamond of face buttons, and the L1/R1/L2 labels).
Textures (oid, 32x32 / 32x16 DXT5 with mips; only level 0 is replaced, the port uploads it without mips):
  button_icon_triangle/circle/square/x.psd  dark round button + symbol -> same button + key letter E / C / F / Space
  button_triangle/circle/square/x.tga        coloured symbol only       -> coloured key letter
  button_l1 / button_r1 / button_l2.psd      dark label "L1" ...       -> "RMB" / "LMB" / "Q"
Bindings = cellPad.c: Cross Space, Circle C, Square F, Triangle E, L1 right mouse, R1 left mouse, L2 Q.
Output: "IFKBMIC2", u32 n, then n x { u32 w, h, fmt(0x88), nbytes, tex_csum(original) ; original L0 ; KBM L0 }.
The drawing functions are shared with make_prompt_sets.py (Xbox / Nintendo / Steam Deck prompt sets).
usage: python make_kbm_prompts.py <core.xpp> <out.bin> [preview.png]"""
import sys, struct, numpy as np
from PIL import Image, ImageDraw, ImageFont
from kbm_glyphs_dxt import dxt5_decode, dxt5_encode, tex_csum
FONT = (lambda os: next((p for p in (                     # KBM_FONT overrides; Windows falls back to Arial Bold
    os.environ.get('KBM_FONT'),
    '/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf',
    os.path.join(os.environ.get('LOCALAPPDATA', ''), 'Microsoft', 'Windows', 'Fonts', 'DejaVuSans-Bold.ttf'),
    os.path.join(os.environ.get('WINDIR', 'C:\\Windows'), 'Fonts', 'DejaVuSans-Bold.ttf'),
    os.path.join(os.environ.get('WINDIR', 'C:\\Windows'), 'Fonts', 'arialbd.ttf'),
    '/System/Library/Fonts/Supplemental/Arial Bold.ttf',
) if p and os.path.isfile(p)), 'DejaVuSans-Bold.ttf'))(__import__('os'))
d = b''
BASE = 0
S = 8
def load_xpp(path):
    """read core.xpp (a PACK archive); BASE = its data section (0x13490 for the European core.xpp)"""
    global d, BASE
    d = open(path, 'rb').read()
    assert d[:4] == b'PACK'
    BASE = struct.unpack('>I', d[0x28:0x2C])[0]
def tex(oid):
    k = 0
    while True:
        k = d.find(struct.pack('>I', oid), k + 1)
        if k < 0: raise SystemExit('texture %d not found' % oid)
        w, h = struct.unpack('>II', d[k+4:k+12])
        if w in (16, 32, 64) and h in (16, 32, 64):
            off, fmt = struct.unpack('>II', d[k+0x20:k+0x28])
            assert (fmt >> 8) & 0xFF == 0x88
            nb = (w // 4) * (h // 4) * 16
            return w, h, d[BASE+off:BASE+off+nb]
def font(px): return ImageFont.truetype(FONT, px)
def put_text(dr, box, txt, col, maxfrac=0.62, outline=None, wfrac=0.86):
    x0, y0, x1, y1 = box; px = int((y1 - y0) * maxfrac)
    while px > 6:
        f = font(px); b = dr.textbbox((0, 0), txt, font=f)
        if b[2]-b[0] <= (x1-x0)*wfrac and b[3]-b[1] <= (y1-y0)*maxfrac: break
        px -= 1
    tw, th = b[2]-b[0], b[3]-b[1]
    pos = (x0 + (x1-x0-tw)/2 - b[0], y0 + (y1-y0-th)/2 - b[1])
    if outline: dr.text(pos, txt, font=f, fill=col, stroke_width=max(1, px//9), stroke_fill=outline)
    else: dr.text(pos, txt, font=f, fill=col)
def spacebar(dr, box, col, w8):
    x0, y0, x1, y1 = box; cx = (x0+x1)/2; cy = (y0+y1)/2; hw = (x1-x0)*0.30; hh = (y1-y0)*0.12
    pts = [(cx-hw, cy-hh), (cx-hw, cy+hh), (cx+hw, cy+hh), (cx+hw, cy-hh)]
    dr.line(pts, fill=col, width=w8, joint='curve')
def sym_color(img):
    """the symbol's colour: most saturated opaque pixels"""
    p = img.reshape(-1, 4).astype(float); a = p[:, 3] > 160
    rgb = p[a, :3]; sat = rgb.max(1) - rgb.min(1)
    top = rgb[sat >= np.percentile(sat, 90)]
    return tuple(int(v) for v in top.mean(0)) + (255,)
def round_button(img, letter, col=None):
    """button_icon_*: keep the round dark button and its rim, repaint the face, draw the key (col: default = the symbol's colour)"""
    H, W = img.shape[:2]
    col = col or sym_color(img)
    p = img.reshape(-1, 4).astype(float); sat = p[:, :3].max(1) - p[:, :3].min(1)
    darkpix = p[(p[:, 3] > 200) & (sat < 30) & (p[:, :3].mean(1) < 70), :3]
    face = tuple(int(v) for v in np.median(darkpix, 0)) + (255,)
    big = Image.fromarray(img).resize((W*S, H*S), Image.LANCZOS); dr = ImageDraw.Draw(big)
    ys, xs = np.nonzero(img[..., 3] > 128); cx, cy = (xs.min()+xs.max()+1)/2*S, (ys.min()+ys.max()+1)/2*S
    r = (min(xs.max()-xs.min(), ys.max()-ys.min())+1)/2*S*0.80
    dr.ellipse([cx-r, cy-r, cx+r, cy+r], fill=face)
    box = (cx-r, cy-r, cx+r, cy+r)
    if letter == ' ': spacebar(dr, box, col, int(S*2.6))
    else: put_text(dr, box, letter, col, 0.66)
    return np.array(big.resize((W, H), Image.LANCZOS))
def symbol(img, letter, col=None):
    """button_*.tga: coloured symbol on transparent -> coloured key letter"""
    H, W = img.shape[:2]; col = col or sym_color(img)
    big = Image.new('RGBA', (W*S, H*S), col[:3] + (0,)); dr = ImageDraw.Draw(big)
    box = (W*S*0.08, H*S*0.08, W*S*0.92, H*S*0.92)
    if letter == ' ': spacebar(dr, box, col, int(S*3.2))
    else: put_text(dr, box, letter, col, 0.78)
    return np.array(big.resize((W, H), Image.LANCZOS))
def label(img, txt, col=None):
    """button_l1 etc.: dark rounded label with white text -> same label, new text"""
    H, W = img.shape[:2]
    p = img.reshape(-1, 4).astype(float)
    darkpix = p[(p[:, 3] > 200) & (p[:, :3].mean(1) < 60), :3]
    face = tuple(int(v) for v in np.median(darkpix, 0)) + (255,)
    big = Image.fromarray(img).resize((W*S, H*S), Image.LANCZOS); dr = ImageDraw.Draw(big)
    ys, xs = np.nonzero(img[..., 3] > 200)
    x0, x1, y0, y1 = (xs.min()+1.2)*S, (xs.max()-0.2)*S, (ys.min()+1.2)*S, (ys.max()-0.2)*S
    dr.rounded_rectangle([x0, y0, x1, y1], radius=S*2, fill=face)
    put_text(dr, (x0 - S, y0 - S*0.5, x1 + S, y1 + S*0.5), txt, (255, 255, 255, 255), 0.95, wfrac=1.0)
    return np.array(big.resize((W, H), Image.LANCZOS))
# The same 11 textures have other object IDs in each release's core.xpp (European BCES00220, US BCUS98119); the set
# whose IDs all resolve to textures of the right size is the one used. Order: button_icon_{triangle,circle,square,x}.psd,
# button_{triangle,circle,square,x}.tga, button_{l1,r1,l2}.psd.
OIDS = {
    'BCES00220': (112262, 104863, 109235, 114719, 112638, 105340, 109491, 101272, 105741, 109557, 111504),
    'BCUS98119': (111258, 103276, 107621, 114074, 110278, 102102, 105987, 112883, 103981, 108352, 112381),
}
SIZES = [(32, 32)] * 8 + [(32, 16)] * 3
def kbm_kinds(b=None):
    """(function, text) for the 11 textures from the keyboard bindings (controls.ini via kbm_bindings, else the port's defaults)"""
    import kbm_bindings
    b = b or kbm_bindings.load(); s = kbm_bindings.short
    return [(round_button, s(b['triangle'])), (round_button, s(b['circle'])), (round_button, s(b['melee'])), (round_button, s(b['jump'])),
            (symbol, s(b['triangle'])), (symbol, s(b['circle'])), (symbol, s(b['melee'])), (symbol, s(b['jump'])),
            (label, s(b['aim'])), (label, s(b['fire'])), (label, s(b['l2']))]
KBM_KINDS = kbm_kinds()
def has_tex(oid, wh):
    try: return tex(oid)[:2] == wh
    except SystemExit: return False
def region():
    r = next((r for r, ids in OIDS.items() if all(has_tex(o, wh) for o, wh in zip(ids, SIZES))), None)
    if not r: raise SystemExit('make_kbm_prompts: this core.xpp holds none of the known button-prompt texture sets')
    return r
def build(jobs, out_path, preview_path=None):
    """jobs: [(oid, fn, text, colour-or-None)] -> IFKBMIC2 file; preview: originals on top, replacements below, 6x"""
    out = b''; prev = []
    for oid, fn, t, col in jobs:
        w, h, raw = tex(oid)
        vis = dxt5_decode(raw, w, h)[::-1]                    # stored bottom-up
        new = fn(vis.copy(), t, col)
        enc = dxt5_encode(np.ascontiguousarray(new[::-1]))
        assert len(enc) == len(raw)
        out += struct.pack('<5I', w, h, 0x88, len(raw), tex_csum(raw, len(raw))) + raw + enc
        prev.append((vis, dxt5_decode(enc, w, h)[::-1]))
    open(out_path, 'wb').write(b'IFKBMIC2' + struct.pack('<I', len(jobs)) + out)
    print('wrote', out_path, len(jobs), 'textures')
    if preview_path:
        Z = 6; tiles = []
        for a, b in prev:
            col = []
            for im in (a, b):
                bg = np.full(im.shape[:2] + (3,), 70.0); al = im[..., 3:4] / 255.0
                col.append(Image.fromarray((im[..., :3]*al + bg*(1-al)).astype(np.uint8)).resize((im.shape[1]*Z, im.shape[0]*Z), Image.NEAREST))
            tiles.append(col)
        Wt = sum(c[0].width + 10 for c in tiles); Ht = 2 * 32 * Z + 10
        c = Image.new('RGB', (Wt, Ht), (30, 30, 30)); x = 0
        for t in tiles: c.paste(t[0], (x, 0)); c.paste(t[1], (x, 32*Z + 10)); x += t[0].width + 10
        c.save(preview_path)
def main():
    load_xpp(sys.argv[1])
    R = region(); print('release', R)
    build([(oid, fn, t, None) for oid, (fn, t) in zip(OIDS[R], KBM_KINDS)], sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else None)
if __name__ == '__main__':
    main()
