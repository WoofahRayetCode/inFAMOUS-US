"""Rebase the EU PPU patches onto the US lift. Writes out/us_final and report files."""
import re, sys, os, json, bisect, collections

R, S = sys.argv[1], sys.argv[2]
PD = R + '/installer/patches/infamous-recomp/recompiled/'
OUT = S + '/out/us_final/'
os.makedirs(OUT, exist_ok=True)


def rd(p):
    return open(p, encoding='utf8', errors='surrogateescape', newline='').read()


ADDR = re.compile(r'(?:func_|loc_)[0-9A-Fa-f]{8}|0x0*[0-9A-Fa-f]{5,8}(?![0-9A-Za-z_])|0x0*[0-9A-Fa-f]{5,8}(?=[uU]\b)')
TOC = re.compile(r'(ctx->gpr\[2\] \+ )-?0x[0-9A-Fa-f]+')


def norm(l):
    l = l.rstrip('\r\n')
    l = re.sub(r'[^\x00-\x7f]', '', l)
    l = TOC.sub(r'\1@', l)
    return ADDR.sub('@', l).strip()


def val(t):
    return int(re.sub(r'^(func_|loc_|0x)', '', t), 16)


names = [f'ppu_recomp_{i:03d}.cpp' for i in range(5)]
files = {n: rd(S + '/out/recompiled/' + n).split('\n') for n in names}
stream, starts = [], {}
for n in names:
    starts[n] = len(stream)
    stream += [(n, i) for i in range(len(files[n]))]
N = [norm(files[n][i]) for n, i in stream]
idx = collections.defaultdict(list)
for p, x in enumerate(N):
    if len(x) > 10:
        idx[x].append(p)


def hunks(name):
    L = rd(PD + name + '.patch').split('\n')
    i = 0
    out = []
    while i < len(L) and not L[i].startswith('@@'):
        i += 1
    while i < len(L):
        st = int(re.match(r'@@ -(\d+)', L[i])[1])
        i += 1
        b = []
        while i < len(L) and not L[i].startswith('@@'):
            b.append(L[i].rstrip('\r'))
            i += 1
        while b and b[-1] == '':
            b.pop()
        out.append((st, b))
    return out


HDR = 262
headers = collections.defaultdict(list)
amap = collections.Counter()
unpl, placed = [], []
stats = collections.Counter()
status = {}
cursor = 0
for n in names:
    drift = 0
    cursor = starts[n]
    for st, b in hunks(n):
        oldb = [x for x in b if x[:1] in (' ', '-')]
        old = [l[1:] for l in oldb]
        nold = [norm(x) for x in old]
        dels = [k for k, l in enumerate(oldb) if l[:1] == '-']
        if st < HDR:
            headers[n].append((st, b))
            stats['header'] += 1
            continue
        exp = st + starts[n] + drift
        an = sorted((len(idx[x]), k) for k, x in enumerate(nold) if len(x) > 10 and x in idx)[:4]
        cands = set()
        for c, k in an:
            ps = idx[nold[k]]
            c0 = bisect.bisect_left(ps, max(cursor, exp - 6000) + k)
            for p in ps[c0:c0 + 60]:
                if cursor <= p - k <= exp + 6000:
                    cands.add(p - k)
        best = None
        for p in cands:
            if p + len(nold) > len(N):
                continue
            miss = sum(N[p + j] != nold[j] for j in range(len(nold)))
            dm = sum(N[p + j] != nold[j] for j in dels)
            sc = (dm, miss, abs(p - exp))
            if best is None or sc < best[0]:
                best = (sc, p)
        lim = max(1, len(nold) // 4)
        if best is None or best[0][0] > 0 or best[0][1] > lim:
            # fallback: anywhere in the stream, but only a unique, fully matching context
            g = set()
            for c, k in an:
                for q in idx[nold[k]][:400]:
                    if q - k >= 0 and q - k + len(nold) <= len(N) and all(N[q - k + j] == nold[j] for j in range(len(nold))):
                        g.add(q - k)
            if len(g) == 1:
                best = ((0, 0, 0), g.pop())
                stats['global'] += 1
        if best is None or best[0][0] > 0 or best[0][1] > lim:
            unpl.append({'file': n, 'line': st, 'body': b})
            status[(n, st)] = 'UNPLACED'
            stats['unplaced'] += 1
            continue
        p = best[1]
        stats['exact' if best[0][1] == 0 else 'fuzzy'] += 1
        cursor = p + len(nold)
        drift = p - st - starts[n]
        for j, x in enumerate(old):
            u = files[stream[p + j][0]][stream[p + j][1]]
            ea, ua = ADDR.findall(x), ADDR.findall(u)
            if len(ea) == len(ua):
                for a, c in zip(ea, ua):
                    amap[(a, c)] += 1
        placed.append((p, b, st, n))
        status[(n, st)] = ('exact' if best[0][1] == 0 else 'fuzzy%d' % best[0][1], p)
print(dict(stats))
json.dump({'unplaced': unpl}, open(S + '/unplaced.json', 'w', encoding='utf8', errors='surrogateescape'), ensure_ascii=True)

pairs = collections.defaultdict(collections.Counter)
for (a, c), v in amap.items():
    pairs[val(a)][val(c)] += v
code = sorted((a, max(cn, key=cn.get)) for a, cn in pairs.items() if 0x10000 <= a < 0x606000)
K = [a for a, _ in code]


def xlate(a):
    if a in pairs:
        return max(pairs[a], key=pairs[a].get), 'seen'
    if 0x10000 <= a < 0x606000:
        i = bisect.bisect(K, a) - 1
        if i >= 0:
            return a + code[i][1] - code[i][0], 'near'
    return None, 'unmapped'


data = {hex(a): {hex(k): v for k, v in c.items()} for a, c in pairs.items() if a >= 0x606000}
json.dump(data, open(S + '/data_pairs.json', 'w'), indent=0)
json.dump({hex(a): hex(u) for a, u in code}, open(S + '/code_map.json', 'w'), indent=0)
print('addr pairs', len(pairs), 'code', len(code), 'data', len(data), 'code deltas', len({u - a for a, u in code}))

tr = collections.Counter()
xl = []


def fixplus(l, local):
    def f(m):
        t = m.group(0)
        a = val(t)
        if a in local:
            u, how = local[a], 'local'
        else:
            u, how = xlate(a)
            if u is None:
                tr['unmapped'] += 1
                xl.append(('unmapped', t, l.strip()[:110]))
                return t
        tr[how] += 1
        if how != 'local' and a != u:
            xl.append((how, t, hex(u), l.strip()[:90]))
        pre = re.match(r'func_|loc_|0x', t).group(0)
        return pre + f'{u:08X}' if pre != '0x' else '0x' + f'{u:0{len(t) - 2}X}'
    return ADDR.sub(f, l)


apply = collections.defaultdict(list)
_sem = {(h['file'][:-6], int(re.match(r'@@ -(\d+)', h['hd'])[1])) for h in json.load(open(S + '/semantic_hunks.json'))}
for p, b, st, n in placed:
    if (n, st) not in _sem:
        continue
    i = p
    local = {}
    for l in b:
        if l[:1] in (' ', '-'):
            u = files[stream[i][0]][stream[i][1]]
            ea, ua = ADDR.findall(l[1:]), ADDR.findall(u)
            if len(ea) == len(ua):
                for a, c in zip(ea, ua):
                    local[val(a)] = val(c)
            i += 1
    i = p
    res = []
    for l in b:
        k = l[:1]
        if k == ' ':
            res.append(('keep', i))
            i += 1
        elif k == '-':
            res.append(('del', i))
            i += 1
        elif k == '+':
            res.append(('add', fixplus(l[1:], local)))
    apply[stream[p][0]].append((p, res))

out = {n: list(files[n]) for n in names}
for n, lst in apply.items():
    lines = files[n]
    s0 = starts[n]
    o = []
    at = 0
    for p, res in sorted(lst, key=lambda x: x[0]):
        loc = p - s0
        assert loc >= at, f'overlapping semantic hunks in {n} at line {loc}'
        if loc > at:
            o += lines[at:loc]
            at = loc
        eol = '\r' if lines[loc].endswith('\r') else ''
        for op, v in res:
            if op == 'keep':
                if v - s0 >= at:
                    o.append(lines[v - s0])
                    at = v - s0 + 1
            elif op == 'del':
                at = v - s0 + 1
            else:
                o.append(v + eol)
    o += lines[at:]
    out[n] = o

all_d = []
seen = set()
for n in names:
    for st, b in headers[n]:
        for l in b:
            if l[:1] == '+' and l[1:].startswith('extern') and l[1:] not in seen:
                seen.add(l[1:])
                all_d.append(l[1:])
print('header decls', len(all_d), {n: len(headers[n]) for n in names})
json.dump([{'file': n, 'line': st, 'body': b} for n in names for st, b in headers[n]],
          open(S + '/header_hunks.json', 'w', encoding='utf8', errors='surrogateescape'), ensure_ascii=True)
for n in names:
    lines = out[n]
    eol = '\r' if lines[0].endswith('\r') else ''
    k = next(i for i, l in enumerate(lines) if l.startswith('extern "C" void ps3_indirect_call'))
    out[n] = lines[:k + 1] + [d + eol for d in all_d] + lines[k + 1:]
    open(OUT + n, 'w', encoding='utf8', errors='surrogateescape', newline='').write('\n'.join(out[n]))
print('translated', dict(tr))
json.dump(xl, open(S + '/xlate_log.json', 'w', encoding='utf8', errors='surrogateescape'), ensure_ascii=True, indent=0)

STV = re.compile(r'(\{ uint64_t ea = .* & ~0xFULL; )memcpy\(vm_base \+ \(uint32_t\)ea, &ctx->vr\[(\d+)\], 16\);( \})')
for n in names:
    out[n] = [STV.sub(r'\1ppu_stvx16((uint32_t)ea, &ctx->vr[\2]);\3', l) for l in out[n]]
    open(OUT + n, 'w', encoding='utf8', errors='surrogateescape', newline='').write('\n'.join(out[n]))
sem = json.load(open(S + '/semantic_hunks.json'))
FN = re.compile(r'^void (func_[0-9A-F]{8})\(')
rep = []
for h in sem:
    n = h['file'][:-6]
    st = int(re.match(r'@@ -(\d+)', h['hd'])[1])
    r = status.get((n, st), 'header')
    fn = ''
    if isinstance(r, tuple):
        c, i = stream[r[1]]
        while i > 0 and not FN.match(files[c][i]):
            i -= 1
        m = FN.match(files[c][i])
        fn = c[11:14] + ' ' + (m[1] if m else '?')
    rep.append({'n': n, 'st': st, 'status': r if isinstance(r, str) else r[0], 'fn': fn, 'body': h['body']})
json.dump(rep, open(S + '/semantic_report.json', 'w', encoding='utf8', errors='surrogateescape'), ensure_ascii=True)
print('semantic status', collections.Counter(x['status'] for x in rep))
pat = re.compile(r'memcpy\(vm_base \+ \(uint32_t\)ea, &ctx->vr\[')
eu_left = sum(1 for n in names for l in rd(PD + n + '.patch').split('\n') if l[:1] == ' ' and pat.search(l))
print('stvx memcpy left: EU context', eu_left,
      'US after', sum(1 for n in names for l in out[n] if pat.search(l)),
      'US before', sum(1 for n in names for l in files[n] if pat.search(l)))
