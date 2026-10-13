import glob,re,sys
for d in sys.argv[1:]:
    n=bad=0
    for f in sorted(glob.glob(d+'/*.prompt.txt')):
        p=open(f,encoding='utf-8').read(); n+=1
        vas=re.findall(r'- (.*?) — вассал; сюзерен — (.*?) \(с',p)
        wars=re.findall(r'- Война: (.*?) и (.*?) — бои',p)
        for v,o in vas:
            if any({a,b}=={v,o} for a,b in wars): bad+=1; break
    print(d.split('/')[-1], 'prompts',n,'vassal+war same pair',bad)
