"""Step 3: the 60 Sonnet chronicles (new prompts, own book and stop-list) against the memory
measurement (#262, glm-5, old prompts). Usage: python3 -I step3.py <sonnet dir> <old dir> <out.md>
"""
import collections, glob, os, re, sys

SEASON_DATE = re.compile(r'^\W*(весн|лет[оа]\b|летом|осен|зим|в лето господне|год\b|года\b|в год|в первые месяцы|в первой половине|во второй половине|первая половина|вторая половина|\d|тысяча|anno)', re.I)
SENT = re.compile(r'(?<=[.!?…])\s+')

ACTORS = {  # formal name start -> stems the chronicles use
    'Венецианская': r'Венец|венециан|Светлейш', 'Флорентийская': r'Флорен|флорентий|Медичи|Лоренцо',
    'Неаполитанское': r'Неапол|неаполитан|Ферранте', 'Папская': r'Пап|пап|Рим|курия|курии|Сикст|понтиф',
    'Сицилийское': r'Сицил', 'Миланское': r'Милан|милан|Сфорц|Лодовико|Моро', 'Генуэзская': r'Генуе|Генуя|Геную|Генуэ|генуэз',
    'Сиенская': r'Сиен|сиен', 'Болонская': r'Болон|болон|Бентивол', 'Феррарское': r'Ферра|феррар|Эсте',
    'Мантуанское': r'Мантуя|Мантуи|Мантую|Мантуе|мантуан|Гонзаг', 'Урбинское': r'Урбин|урбин|Монтефельтр',
    'Савойское': r'Савой|савой', 'Франция': r'Франц|франц',
}
LABELS = {'борьба партий': r'парти|фракци|раскол|распр', 'баронская фронда': r'барон|фронд', 'кумовство при дворе': r'кумовств|непот|родн|племянник',
          'кризис регентства': r'регентств|регент', 'упадок': r'упад'}
CONTINUE = re.compile(r'продолжа|всё ещё|все еще|по-прежнему|снова|опять|который год|не первый|затяж|давн|годами|лет\b|уже не|так и не|не утиха|тянет', re.I)
RESTART = re.compile(r'вспыхнул|разгорел|началась|начался|впервые|открыл[аи]? войну|объявил[аи]? войну', re.I)


def stem(formal):
    for k, v in ACTORS.items():
        if formal.strip().startswith(k):
            return v
    return re.escape(formal.strip().split()[0][:5])


def words(t):
    return [w.lower() for w in re.findall(r"[A-Za-zА-Яа-яЁё0-9-]+", t)]


def body(text):
    return '\n'.join(l for l in text.splitlines() if not l.lstrip().startswith('#')).strip()


def load(d, keep=None):
    games = collections.defaultdict(list)
    for f in sorted(glob.glob(os.path.join(d, '*.text.txt'))):
        b = os.path.basename(f)[:-9]
        world, seed, idx, tick = b.split('_')
        t = int(tick[1:])
        if keep is not None and t not in keep.get((world, seed), ()):  # same windows as the other run
            continue
        prompt = open(f[:-9] + '.prompt.txt', encoding='utf-8').read() if os.path.exists(f[:-9] + '.prompt.txt') else ''
        games[(world, seed)].append((t, body(open(f, encoding='utf-8').read()), prompt))
    for g in games.values():
        g.sort()
    return games


def ngrams(ws, n):
    return {' '.join(ws[i:i + n]) for i in range(len(ws) - n + 1)}


def repeats(games):
    out = {}
    for key, chron in games.items():
        texts = [c[1] for c in chron if len(c[1]) > 200]
        g4 = collections.Counter()
        g5 = collections.Counter()
        for t in texts:
            g4.update(ngrams(words(t), 4))
            g5.update(ngrams(words(t), 5))
        pair = []
        for a, b in zip(texts, texts[1:]):
            na, nb = ngrams(words(a), 4), ngrams(words(b), 4)
            pair.append(100 * len(na & nb) / max(1, len(nb)))
        firsts = [SENT.split(t, 1)[0] for t in texts]
        open4 = collections.Counter(' '.join(words(f)[:4]) for f in firsts)
        out[key] = dict(
            n=len(texts),
            rep4=sum(1 for v in g4.values() if v >= 3),
            rep5=[(k, v) for k, v in g5.most_common() if v >= 3][:6],
            pair_max=max(pair) if pair else 0, pair_mean=sum(pair) / len(pair) if pair else 0,
            date_open=sum(1 for f in firsts if SEASON_DATE.search(f)),
            open_rep=sum(v - 1 for v in open4.values() if v > 1),
            open_rep_list=[(k, v) for k, v in open4.items() if v > 1],
        )
    return out


def block(prompt, title, end_titles=('===',)):
    i = prompt.find(title)
    if i < 0:
        return []
    rest = prompt[i + len(title):]
    lines = []
    for l in rest.splitlines()[1:]:
        if l.startswith('==='):
            break
        lines.append(l)
    return lines


def sub(lines, head):
    out, on = [], False
    for l in lines:
        if not l.startswith('- ') and l.strip().endswith(':'):
            on = l.strip().startswith(head)
            continue
        if on and l.startswith('- '):
            out.append(l[2:])
    return out


def stoplist(prompt):
    i = prompt.find('НЕ ПОВТОРЯЙ обороты')
    if i < 0:
        return []
    out = []
    for l in prompt[i:].splitlines()[1:]:
        m = re.match(r'- «(.*)»', l)
        if not m:
            break
        out.append(m.group(1))
    return out


def has(rx, text):
    return re.search(rx, text) is not None


def analyse_layers(games, flags):
    c = collections.Counter()
    story_runs = collections.defaultdict(list)  # (game, story) -> [(tick, mentioned, sentence)]
    for key, chron in games.items():
        for tick, text, prompt in chron:
            if len(text) < 200:
                continue
            sents = SENT.split(text)
            # stop-list
            sl = stoplist(prompt)
            tw = ' ' + ' '.join(words(text)) + ' '
            c['stop_phrases'] += len(sl)
            for p in sl:
                if ' ' + p + ' ' in tw:
                    c['stop_violations'] += 1
                    flags.append(f'{key} t{tick} STOP-LIST REPEATED: «{p}»')
            annals = block(prompt, '=== ЛЕТОПИСЬ МИРА')
            rel = sub(annals, 'Отношения сейчас')
            stories = sub(annals, 'Открытые сюжеты')
            chrono = sub(annals, 'Хронология партии')
            dossier = sub(annals, 'Ведущие державы')
            # relations
            for l in rel:
                if l.startswith('Союз:'):
                    c['rel_alliance'] += 1
                    if has(r'[Лл]иг[аиуе]|[Лл]игой|союз|коалиц', text):
                        c['rel_alliance_used'] += 1
                m = re.match(r'(.*) — вассал; сюзерен — (.*) \(с', l)
                if m:
                    c['rel_vassal'] += 1
                    v, o = stem(m.group(1)), stem(m.group(2))
                    if any(has(v, s) and has(r'подчин|вассал|покор|сюзерен|склонил|признал|под рук|присяг', s) for s in sents):
                        c['rel_vassal_used'] += 1
                    # contradiction: vassal again under pressure/at war, without a word of submission
                    for s in sents:
                        if has(v, s) and has(o, s) and has(r'давит|давлен|осад|наступ|воюет|война с|бои с', s) and not has(r'подчин|вассал|покор|сюзерен|склонил|признал|бывш|прежн|годами|лет', s):
                            c['contra_vassal'] += 1
                            flags.append(f'{key} t{tick} VASSAL-UNDER-PRESSURE?: {s.strip()[:240]}')
                            break
            # open stories
            for l in stories:
                m = re.match(r'Война: (.*) и (.*) — бои идут с (\d+)', l)
                if m:
                    kind, rx = 'war', (stem(m.group(1)), stem(m.group(2)))
                    sid = f'war {m.group(1)[:12]}–{m.group(2)[:12]}'
                elif l.startswith('Противостояние:'):
                    kind, rx, sid = 'league', (r'[Лл]иг[аиуе]|[Лл]игой|коалиц|союз',), 'league'
                elif 'терпит поражение' in l:
                    a = l.split(' терпит')[0]
                    kind, rx, sid = 'defeats', (stem(a), r'поражен|разбит|теря|отступ|бит'), f'defeats {a[:14]}'
                else:
                    m = re.match(r'(.*): (.*) — с (\d+)', l)
                    if not m:
                        continue
                    lab = LABELS.get(m.group(2).strip(), re.escape(m.group(2)[:5]))
                    kind, rx, sid = 'crisis', (stem(m.group(1)), lab), f'crisis {m.group(1)[:12]}: {m.group(2)}'
                c[f'story_{kind}'] += 1
                hit = next((s for s in sents if all(has(r, s) for r in rx)), None)
                if hit:
                    c[f'story_{kind}_used'] += 1
                story_runs[(key, sid)].append((tick, hit))
            # chronology: a past dated fact (an earlier year named, or how long ago)
            year = int(re.search(r'(\d{4}) год', prompt[prompt.find('=== СОСТОЯНИЕ МИРА'):] or '').group(1)) if re.search(r'(\d{4}) год', prompt[prompt.find('=== СОСТОЯНИЕ МИРА'):] or '') else None
            if chrono:
                c['chrono'] += 1
                past_years = [int(y) for y in re.findall(r'\b(14\d\d)\b', text) if year and int(y) < year]
                if past_years or has(r'(\d+|два|три|четыре|пять|шесть|семь|восемь|девять|десять|дюжин)\w* (лет|год\w*) (назад|тому)|лет как|с тех пор|минувш|десятилет', text):
                    c['chrono_used'] += 1
            # dossier
            for l in dossier:
                a, trends = l.split(': ', 1)
                for tr in trends.split('; '):
                    c['dossier'] += 1
                    if 'армия' in tr:
                        t_rx = r'арми|войск|ополчен|солдат|кондотьер|гарнизон'
                    elif 'единство' in tr:
                        t_rx = r'единств|сплоч|раздор|смут|мир внутри|спокой'
                    elif 'власть' in tr:
                        t_rx = r'власт|законн|прав|трон|корон'
                    elif 'хозяйство' in tr or 'богат' in tr or 'казн' in tr:
                        t_rx = r'казн|богат|торгов|хозяйств|золот|доход'
                    elif 'люд' in tr or 'насел' in tr:
                        t_rx = r'люд|насел|народ|жител|подданн'
                    else:
                        t_rx = r'$^'
                    if any(has(stem(a), s) and has(t_rx, s) for s in sents):
                        c['dossier_used'] += 1
            # player policy (aggressive only has any)
            pol = block(prompt, '=== ПОЛИТИКА ИГРОКА')
            older = [l for l in pol if l.startswith('- ') and re.search(r'\d{4}', l)]
            recent_none = any('Решений за последние два года не было' in l for l in pol)
            if older:
                c['policy_old'] += 1
                if has(r'ополчен|Генуе|Генуя|Геную|генуэз|барон|банковск', text):
                    c['policy_old_used'] += 1
                if recent_none:
                    for s in sents:
                        if has(r'ополчен|баронов|бароны|Генуе|Генуя|Геную|банковск', s) and has(r'ныне|нынешн|этой (весн|осен|зим)|этим летом|в этом году|в эту (весну|осень|зиму)|в это полугодие|этого полугодия|сейчас|только что|недавно', s):
                            c['contra_old_as_new'] += 1
                            flags.append(f'{key} t{tick} OLD-ACTION-AS-NEW?: {s.strip()[:240]}')
                            break
            # alliance on the wrong side: Florence with Milan while the league is against Milan
            if any('общий враг — Миланское' in l for l in rel):
                for s in sents:
                    if has(r'Флоренц|Медичи|Лоренцо', s) and has(r'союз|альянс|дружб', s) and has(r'Милан|Сфорц|Лодовико', s) and not has(r'против|лиг|враг|бывш|прежн|некогда|когда-то|разорв|распал', s):
                        c['contra_florence_milan'] += 1
                        flags.append(f'{key} t{tick} FLORENCE-MILAN-ALLY?: {s.strip()[:240]}')
                        break
    # continuity: a story told in two or more chronicles — later mentions as continuing or restarting
    for (key, sid), run in story_runs.items():
        told = [(t, s) for t, s in run if s]
        c['runs'] += 1
        if len(run) >= 2:
            c['runs_multi'] += 1
            if len(told) >= 2:
                c['runs_told_again'] += 1
        for t, s in told[1:]:
            c['later_mentions'] += 1
            if CONTINUE.search(s):
                c['later_continue'] += 1
            if RESTART.search(s) and not CONTINUE.search(s):
                c['later_restart'] += 1
                flags.append(f'{key} t{t} STORY-RESTART? [{sid}]: {s.strip()[:240]}')
    return c, story_runs


def main():
    sonnet, old, out = sys.argv[1], sys.argv[2], sys.argv[3]
    gs = load(sonnet)
    keep = {k: {t for t, _, _ in v} for k, v in gs.items()}
    go = load(old, keep)
    rs, ro = repeats(gs), repeats(go)
    flags = []
    c, runs = analyse_layers(gs, flags)
    L = ['# Шаг 3 — разбор (сырые числа)', '', '## Повторы: Sonnet (новые промпты) против #262 (glm-5, старые)', '',
         '| партия | хроник S/G | 4-граммы в ≥3 хрониках S/G | пересечение соседей, ср./макс. % S | G | зачин с даты S/G | повтор зачина S/G |', '|---|---|---|---|---|---|---|']
    tot = collections.Counter()
    for k in sorted(rs):
        a, b = rs[k], ro.get(k, {})
        L.append(f"| {k[0]} {k[1]} | {a['n']}/{b.get('n')} | {a['rep4']}/{b.get('rep4')} | {a['pair_mean']:.1f}/{a['pair_max']:.1f} | {b.get('pair_mean', 0):.1f}/{b.get('pair_max', 0):.1f} | {a['date_open']}/{b.get('date_open')} | {a['open_rep']}/{b.get('open_rep')} |")
        for f in ('n', 'rep4', 'date_open', 'open_rep'):
            tot['s' + f] += a[f]
            tot['g' + f] += b.get(f, 0)
    L.append(f"| **всего** | {tot['sn']}/{tot['gn']} | {tot['srep4']}/{tot['grep4']} | | | {tot['sdate_open']}/{tot['gdate_open']} | {tot['sopen_rep']}/{tot['gopen_rep']} |")
    L += ['', '### 5-граммы в ≥3 хрониках партии', '']
    for name, r in (('Sonnet', rs), ('glm-5 #262', ro)):
        for k in sorted(r):
            if r[k]['rep5']:
                L.append(f"- {name} {k[0]} {k[1]}: " + '; '.join(f'«{p}» ×{v}' for p, v in r[k]['rep5']))
    L += ['', '### Повторы зачинов (4 слова)', '']
    for name, r in (('Sonnet', rs), ('glm-5 #262', ro)):
        for k in sorted(r):
            if r[k]['open_rep_list']:
                L.append(f"- {name} {k[0]} {k[1]}: " + '; '.join(f'«{p}» ×{v}' for p, v in r[k]['open_rep_list']))
    L += ['', '## Слои летописи и противоречия (Sonnet)', '', '| что | значение |', '|---|---|']
    for k in sorted(c):
        L.append(f'| {k} | {c[k]} |')
    L += ['', '## Флаги сита (читать руками)', ''] + [f'- {f}' for f in flags]
    L += ['', '## Сюжеты по партиям: тик → упомянут ли', '']
    for (key, sid), run in sorted(runs.items()):
        L.append(f"- {key[0]} {key[1]} [{sid}]: " + ' '.join(f"{t}{'+' if s else '·'}" for t, s in run))
    open(out, 'w', encoding='utf-8').write('\n'.join(L) + '\n')
    print('\n'.join(L[:40]))


main()
