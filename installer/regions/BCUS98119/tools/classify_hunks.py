import re,sys,os,glob,collections,json
R,S=sys.argv[1],sys.argv[2]
out={};tot=collections.Counter();sem=[]
sq=lambda l:re.sub(r'\s+',' ',l).strip()
def mech(minus,plus):
    if len(minus)!=len(plus):return None
    kinds=set()
    for m,p in zip(minus,plus):
        if sq(m)==sq(p):kinds.add('ws');continue
        # encoding only
        if m.encode('ascii','ignore')==p.encode('ascii','ignore') or re.sub(r'[^\x00-\x7f]','',m)==re.sub(r'[^\x00-\x7f]','',p):kinds.add('enc');continue
        a=re.fullmatch(r'\s*\{ uint64_t ea = (.*) & ~0xFULL; memcpy\(vm_base \+ \(uint32_t\)ea, &ctx->vr\[(\d+)\], 16\); \}\s*',m)
        b=re.fullmatch(r'\s*\{ uint64_t ea = (.*) & ~0xFULL; ppu_stvx16\(\(uint32_t\)ea, &ctx->vr\[(\d+)\]\); \}\s*',p)
        if a and b and a.groups()==b.groups():kinds.add('stvx');continue
        return None
    return kinds
for f in sorted(glob.glob(R+'/installer/patches/infamous-recomp/recompiled/*.patch')):
    name=os.path.basename(f);lines=open(f,encoding='utf8',errors='surrogateescape').read().split('\n')
    i=0
    while i<len(lines) and not lines[i].startswith('@@'):i+=1
    while i<len(lines):
        hd=lines[i];i+=1;body=[]
        while i<len(lines) and not lines[i].startswith('@@'):body.append(lines[i]);i+=1
        # group change runs
        run_m=[];run_p=[];kinds=set();bad=False
        def flush():
            global run_m,run_p,bad
            if run_m or run_p:
                k=mech(run_m,run_p)
                if k is None:bad=True
                else:kinds.update(k)
            run_m=[];run_p=[]
        for l in body:
            if l.startswith('-'):run_m.append(l[1:])
            elif l.startswith('+'):run_p.append(l[1:])
            else:flush()
        flush()
        if bad:
            tot['semantic']+=1;sem.append({'file':name,'hd':hd,'body':body})
        else:
            for k in kinds or {'empty'}:tot[k]+=1
print(tot)
json.dump(sem,open(S+'/semantic_hunks.json','w',encoding='utf8',errors='surrogateescape'),ensure_ascii=True)
c=collections.Counter(s['file'] for s in sem);print(c)
print('added/removed in semantic',sum(1 for s in sem for l in s['body'] if l[:1]=='+'),sum(1 for s in sem for l in s['body'] if l[:1]=='-'))
# name each hunk by enclosing func from context
for s in sem[:400]:
    plus=[l[1:].strip() for l in s['body'] if l[:1]=='+'];minus=[l[1:].strip() for l in s['body'] if l[:1]=='-']
    print(s['file'][11:14],s['hd'][:26],f'-{len(minus)} +{len(plus)}',(plus or minus)[0][:90])
