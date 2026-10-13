import glob,re,sys,collections
c=collections.Counter(); uniq=collections.Counter()
for f in sorted(glob.glob(sys.argv[1]+'/*.prompt.txt')):
    p=open(f,encoding='utf-8').read()
    a=p.find('Хронология партии:'); b=p.find('Ведущие державы')
    if a<0: continue
    chrono=p[a:b if b>a else p.find('=== НЕДАВНИЕ')]
    rest=p[:a]+p[(b if b>a else a):]
    items=[]
    for l in chrono.splitlines()[1:]:
        if not l.startswith('- '): continue
        l=l[2:]
        body=re.sub(r'^(\d{4}-е|\d{4} г\.[^—]*—)\s*:?\s*','',l)
        for it in re.split(r'; ',body):
            items.append(it)
    for it in items:
        c['items']+=1
        core=re.sub(r'^(веха|союз|союз обращён[^:]*|решения игрока):\s*','',it).strip()
        parts=[x.strip() for x in core.split(', ')] if it.startswith('решения игрока') else [core]
        dup=all(x[:25] in rest for x in parts if x)
        if dup: c['dup']+=1
        else: uniq[core[:70]]+=1
print(c); print(uniq.most_common(15))
