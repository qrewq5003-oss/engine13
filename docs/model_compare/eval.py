"""Step 2 evaluation: the four models on the six games (60 chronicles each).

Usage: python3 -I cmp_eval.py <cmpdir> [<olddir> as a fifth column — the glm-5 run of the memory measurement]
Prints a markdown table per criterion and writes flagged sentences to <cmpdir>/<model>.flags.md for reading.
"""
import os, re, sys, glob, collections

CJK = re.compile(r'[぀-ヿ㐀-鿿가-힯豈-﫿]')
OTHER_SCRIPT = re.compile(r'[֐-׿؀-ۿ฀-๿ऀ-ॿႠ-ჿ]')
LATIN_WORD = re.compile(r'\b[A-Za-z][A-Za-z\'-]{2,}\b')
# authored Latin that the prompt itself carries (not the model's English)
AUTHORED_LATIN = {'camera', 'degli', 'sposi', 'del', 'della', 'di', 'da', 'il', 'la', 'le', 'de'}
ENGINE_KEY = re.compile(r'\b[a-z]+_[a-z_]+\b|\b(cohesion|legitimacy|military_size|economic_output|external_pressure|treasury|power_projection)\b')
DIGIT_NON_YEAR = re.compile(r'(?<![\d])(?!1[3-6]\d\d(?!\d))\d+')
METRIC_WORDS = re.compile(r'коэффициент|показател[ьяе]|процент|единиц', re.I)
SEASON_DATE = re.compile(r'^\W*(весн|лет[оа]\b|летом|осен|зим|в лето господне|год\b|года\b|в год|в первые месяцы|в первой половине|во второй половине|первая половина|вторая половина|\d|тысяча|anno)', re.I)
SENT = re.compile(r'(?<=[.!?…])\s+')


def words(t):
    return [w.lower() for w in re.findall(r"[A-Za-zА-Яа-яЁё0-9-]+", t)]


def load(d):
    games = collections.defaultdict(list)
    for f in sorted(glob.glob(os.path.join(d, '*.text.txt'))):
        b = os.path.basename(f)[:-9]
        world, seed, idx, tick = b.split('_')
        text = open(f, encoding='utf-8').read()
        facts = open(f[:-9] + '.facts.txt', encoding='utf-8').read() if os.path.exists(f[:-9] + '.facts.txt') else ''
        games[(world, seed)].append((int(tick[1:]), text, facts))
    for g in games.values():
        g.sort()
    return games


def evaluate(d):
    games = load(d)
    r = collections.Counter()
    flags = []
    openings = collections.Counter()
    lengths = []
    for (world, seed), chron in games.items():
        firsts = []
        vassal_since = None
        for tick, text, facts in chron:
            r['n'] += 1
            body = text.strip()
            if len(body) < 200:
                r['empty'] += 1
                continue
            r['written'] += 1
            lengths.append(len(text))
            if CJK.search(body) or OTHER_SCRIPT.search(body):
                r['alien'] += 1
                for m in list(CJK.finditer(body))[:2]:
                    flags.append(f'{world} {seed} t{tick} ALIEN: …{body[max(0, m.start()-30):m.end()+20]}…')
            lat = [w for w in LATIN_WORD.findall(body) if w.lower() not in AUTHORED_LATIN]
            if len(lat) >= 20:
                r['latin_heavy'] += 1
            if lat:
                r['latin'] += 1
                r['latin_words'] += len(lat)
                flags.append(f'{world} {seed} t{tick} LATIN: {", ".join(lat[:6])}')
            if ENGINE_KEY.search(body):
                r['keys'] += 1
                flags.append(f'{world} {seed} t{tick} KEY: {ENGINE_KEY.search(body).group(0)}')
            nd = DIGIT_NON_YEAR.findall(body)
            if nd:
                r['digits'] += 1
                flags.append(f'{world} {seed} t{tick} DIGIT: {nd[:5]}')
            if METRIC_WORDS.search(body):
                r['metric_words'] += 1
            first = SENT.split(body, 1)[0]
            if SEASON_DATE.search(first):
                r['date_opening'] += 1
            w4 = ' '.join(words(first)[:4])
            firsts.append(w4)
            openings[w4] += 1
            # Proxies for contradictions, read by hand afterwards.
            m = re.search(r'vassal: siena \(.*?\) → overlord florence .*?since tick (\d+)', facts)
            if m:
                vassal_since = int(m.group(1))
            sents = SENT.split(body)
            if vassal_since is not None and tick > vassal_since:
                for s in sents:
                    if re.search(r'Сиен', s) and re.search(r'давит|давлен|осад|военн\w* конфликт|наступ|воюет|война с', s) and not re.search(r'подчин|вассал|покор|сюзерен', s):
                        r['siena_after'] += 1
                        flags.append(f'{world} {seed} t{tick} SIENA-AFTER-SUBMISSION: {s.strip()[:220]}')
                        break
            if world == 'aggressive':
                for s in sents:
                    if re.search(r'Флоренц', s) and re.search(r'союз|альянс|дружб', s) and re.search(r'Милан|Сфорц|Лодовико', s) and not re.search(r'против|лиг', s):
                        r['florence_milan_ally'] += 1
                        flags.append(f'{world} {seed} t{tick} FLORENCE-MILAN-ALLY?: {s.strip()[:220]}')
                        break
                acts = re.search(r'player actions so far: \d+ — (.*)', facts)
                last = max([int(x.split('@')[1]) for x in acts.group(1).split(', ') if '@' in x], default=None) if acts else None
                if last is not None and tick - last > 4:
                    for s in sents:
                        if re.search(r'ополчен|баронов|бароны|Генуе|Генуя|Геную', s) and re.search(r'ныне|нынешн|этой (весн|осен|зим)|этим (летом|летом)|в этом году|в эту (весну|осень|зиму)|в это полугодие|этого полугодия|сейчас|только что|недавно', s, re.I):
                            r['old_as_new'] += 1
                            flags.append(f'{world} {seed} t{tick} OLD-ACTION-AS-NEW (last action t{last}): {s.strip()[:220]}')
                            break
        # opening repeats inside the game
        c = collections.Counter(firsts)
        r['opening_repeats'] += sum(v - 1 for v in c.values() if v > 1)
    lengths.sort()
    r['median_len'] = lengths[len(lengths) // 2] if lengths else 0
    return r, flags


def main():
    cmp = sys.argv[1]
    dirs = sorted(d for d in glob.glob(os.path.join(cmp, '*')) if os.path.isdir(d))
    if len(sys.argv) > 2:
        dirs.append(sys.argv[2])
    rows = [
        ('хроник', 'n'), ('пустой ответ (в книгу не попал)', 'empty'), ('написано', 'written'), ('чужая письменность (хроник)', 'alien'), ('латинские слова (хроник / слов)', 'latin'),
        ('из них ≥ 20 латинских слов', 'latin_heavy'), ('ключи движка (хроник)', 'keys'), ('числа кроме годов (хроник)', 'digits'), ('слова-метрики (хроник)', 'metric_words'),
        ('зачин с даты или времени года', 'date_opening'), ('повтор зачина в партии (4 слова)', 'opening_repeats'),
        ('сито: Сиена «под давлением» после подчинения', 'siena_after'), ('сито: Флоренция в союзе с Миланом (aggressive)', 'florence_milan_ally'),
        ('сито: старое решение игрока как новое (aggressive)', 'old_as_new'), ('длина, медиана знаков', 'median_len'),
    ]
    res = {}
    for d in dirs:
        r, flags = evaluate(d)
        name = os.path.basename(d.rstrip('/'))
        res[name] = r
        with open(os.path.join(cmp, name + '.flags.md'), 'w', encoding='utf-8') as f:
            f.write('\n'.join(flags) + '\n')
    names = list(res)
    print('| критерий | ' + ' | '.join(names) + ' |')
    print('|---|' + '---|' * len(names))
    for label, k in rows:
        cells = []
        for n in names:
            v = res[n][k]
            cells.append(f"{v} / {res[n]['latin_words']}" if k == 'latin' else str(v))
        print(f'| {label} | ' + ' | '.join(cells) + ' |')


main()
