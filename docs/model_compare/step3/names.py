import glob,re,sys,collections,os
d=sys.argv[1]
games=collections.defaultdict(list)
for f in sorted(glob.glob(d+'/*.text.txt')):
    b=os.path.basename(f).split('_'); games[(b[0],b[1])].append(f)
tot=collections.Counter()
for g,fs in sorted(games.items()):
    seen=collections.Counter(); first={}
    for f in fs:
        t=open(f,encoding='utf-8').read(); p=open(f[:-9]+'.prompt.txt',encoding='utf-8').read()
        pl=p.lower()
        for m in re.finditer(r'(?<![.!?…]\s)(?<!^)\b([А-ЯЁ][а-яё]{3,})\b',t):
            w=m.group(1)
            if w[:5].lower() in pl: continue
            seen[w]+=1
    multi=[(w,c) for w,c in seen.most_common() if c>=2]
    print(g, 'new capitalised words:',len(seen),'in >=2 places:',multi[:15])
