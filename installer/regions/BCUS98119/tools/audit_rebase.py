"""Finish the US tree (ppu_recomp.h) and audit it against the EU patch set."""
import json, re, sys, os, collections, subprocess
R, S = sys.argv[1], sys.argv[2]
PD = R + '/installer/patches/infamous-recomp/recompiled/'
rd = lambda p: open(p, encoding='utf8', errors='surrogateescape', newline='').read()
names = [f'ppu_recomp_{i:03d}.cpp' for i in range(5)]
base = {n: rd(S + '/out/recompiled/' + n) for n in names}
fin = {n: rd(S + '/out/us_final/' + n) for n in names}
print('final/base sizes', {n[11:14]: (len(fin[n]) - len(base[n])) for n in names})

# --- header: same hunks, same line numbers?
H = rd(S + '/out/recompiled/ppu_recomp.h').split('\n')
P = rd(PD + 'ppu_recomp.h.patch').split('\n')
out, at, i, ok = [], 0, 0, True
while i < len(P):
    m = re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@', P[i])
    if not m:
        i += 1
        continue
    st = int(m[1]) - 1
    oc = int(m[2] or 1)
    nc = int(m[4] or 1)
    i += 1
    out += H[at:st]
    at = st
    so = sn = 0
    while (so < oc or sn < nc) and i < len(P):
        l = P[i]
        i += 1
        t, body = l[:1], l[1:]
        if t == ' ':
            if H[at].rstrip('\r') != body.rstrip('\r'):
                ok = False
                print('header context mismatch at', at + 1, repr(H[at][:60]), repr(body[:60]))
            out.append(H[at])
            at += 1
            so += 1
            sn += 1
        elif t == '-':
            at += 1
            so += 1
        elif t == '+':
            out.append(body + ('\r' if H[0].endswith('\r') and not body.endswith('\r') else ''))
            sn += 1
out += H[at:]
print('header applies with exact context:', ok, 'lines', len(H), '->', len(out))
open(S + '/out/us_final/ppu_recomp.h', 'w', encoding='utf8', errors='surrogateescape', newline='').write('\n'.join(out))

# --- other rewrites: anything left in the old form that EU no longer has?
PATS = {
    'dcbz128 memset': r'memset\(vm_base \+ \(uint32_t\)ea, 0, 128\)',
    'vshift memcpy-first': r'memcpy\(a_, &ctx->vr\[\d+\], 16\); const unsigned s_',
    'atomic lhz/addi/sth': None,
}
euctx = collections.Counter()
for n in names:
    for l in rd(PD + n + '.patch').split('\n'):
        if l[:1] == ' ':
            for k, p in PATS.items():
                if p and re.search(p, l):
                    euctx[k] += 1
for k, p in PATS.items():
    if p:
        print(f'{k}: EU context still old form {euctx[k]}; US base {sum(len(re.findall(p, base[n])) for n in names)}; US final {sum(len(re.findall(p, fin[n])) for n in names)}')
# atomic triple
tri = re.compile(r'ctx->gpr\[(\d+)\] = vm_read16\(([^\n]*)\);\r?\n\s*ctx->gpr\[\1\] = ctx->gpr\[\1\] \+ \(int64_t\)\((-?\d+)\);\r?\n\s*vm_write16\(\2, ctx->gpr\[\1\]\);')
print('atomic triple: US base', sum(len(tri.findall(base[n])) for n in names), 'US final', sum(len(tri.findall(fin[n])) for n in names),
      'ppu_atomic_add16 in final', sum(fin[n].count('ppu_atomic_add16(') for n in names))

# --- what the rewritten + lines say
print('== addresses inserted (translated):')
for e in json.load(open(S + '/xlate_log.json'))[:20]:
    print('  ', [str(x)[:95] for x in e])
HOOK = re.compile(r'ps3_pad_rstick_read\(0x[0-9A-F]+u|ps3_pad_mouse_inject\(|ps3_pad_mouse_cam_spring\(|ps3_taskwait_begin\(\)|infamous_trainer_move_set\(ctx|infamous_trainer_move_pre\(ctx|car_v233_vis\(ctx|car_v233_dscale\(ctx|infamous_anim_lod_skip\(ctx|infamous_anim_lod_note\(ctx|inf_cell_probe\(_cp|infamous_movie_pool')
c = collections.Counter()
for n in names:
    for m in HOOK.finditer(fin[n]):
        c[m[0][:44]] += 1
print('== hooks in final:')
for k, v in sorted(c.items()):
    print('  ', v, k)
# Every EU func_ token in + lines must exist in the US lift
us = set()
for n in names:
    us |= set(re.findall(r'^void (func_[0-9A-F]{8})\(', base[n], re.M))
newtok = collections.Counter()
for n in names:
    a = set(re.findall(r'func_[0-9A-F]{8}\(ctx\)', base[n]))
    b = set(re.findall(r'func_[0-9A-F]{8}\(ctx\)', fin[n]))
    newtok.update(b - a)
print('call targets in final but not in that chunk of base:', list(newtok)[:12], 'all known', all(k[:13] in us for k in newtok))
