# Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.
"""Builds the bundled company / Chinese place lists from Wikidata.

    python scripts/fetch_bundled_lists.py [--out data/lists]
                                       [--stage companies|places|all|refilter] [--refresh]

--stage refilter applies keep_name again to the companies.json already in --out,
without querying Wikidata: run it after changing the filter.

Companies: every entity that is (a subclass of) business, whose country is one
of the target markets and that has enough Wikipedia articles (sitelinks, a
cheap notability proxy; the threshold is per market because Chinese, French
and Spanish companies have fewer wikis). Labels and aliases in en / fr / es / zh.

Places (zh): Chinese provinces, municipalities, autonomous regions, SARs and
prefecture- / county-level cities, labels only (aliases are historical names).
The NER model misses bare city names such as 深圳 in long Chinese text.

Filtering (see keep_name): single words that are ordinary vocabulary in
English, French or Spanish (Apple, Orange, Total), short acronyms and
two-character CJK names unless the company is well known, domain names,
company names that are also place names (Chinese places, US states and common
country names: KFC's alias "Kentucky"), names shorter than 3 Latin / 2 CJK
characters, names longer than 120 characters (vandalised aliases such as
sentences pasted into Wikidata). A name is judged as the app matches it, with the punctuation at
both ends stripped: "'One'", "One+" and "Anti-" are the words One and Anti. Word lists come from the LibreOffice hunspell dictionaries and
jieba's dict.txt. Downloads and SPARQL results are cached under the OS temp
dir; --refresh re-queries Wikidata.
"""
import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
import time
import unicodedata

import requests

SPARQL = 'https://query.wikidata.org/sparql'
UA = 'docudis-list-builder/1.0'

MARKETS = {
    'en': ['Q145', 'Q30', 'Q16', 'Q408', 'Q664'],   # UK, US, Canada, Australia, NZ
    'fr': ['Q142', 'Q31', 'Q39'],                    # France, Belgium, Switzerland
    'zh': ['Q148', 'Q8646', 'Q865'],                 # China, Hong Kong, Taiwan
    'es': ['Q29'],                                   # Spain
}
MIN_SITELINKS = {'en': 5, 'fr': 3, 'es': 3, 'zh': 3}
WELL_KNOWN = 40  # sitelinks needed for two-character CJK company names (红旗, 黑猫 are also words)
BIG_CITY = 3_000_000  # population from which a prefecture-level city may appear without 市
LANGS = ['en', 'fr', 'es', 'zh', 'zh-hans', 'zh-hant', 'zh-cn', 'zh-hk', 'zh-tw']
BUSINESS = 'Q4830453'

# Chinese administrative classes (instance-of), looked up on wikidata.org.
# The second field says when the name may also appear without its 市 / 省
# suffix: "always", only for "big" cities (population >= BIG_CITY, or a name
# of three characters or more) or "never". Bare two-character names of small
# cities (大庆, 安康, 白银, 朝阳) are also ordinary words.
PLACE_CLASSES = {
    'Q1615742': ('province of China', 'always'),
    'Q1208802': ('direct-administered municipality', 'always'),
    'Q57362': ('autonomous region of China', 'always'),
    'Q779415': ('special administrative region of China', 'always'),
    'Q748149': ('prefecture-level city of China', 'big'),
    'Q250811': ('sub-province-level division', 'always'),
    'Q1070990': ('county-level city of China', 'never'),
}

WORDLISTS = {
    'en': 'https://raw.githubusercontent.com/LibreOffice/dictionaries/master/en/en_US.dic',
    'fr': 'https://raw.githubusercontent.com/LibreOffice/dictionaries/master/fr_FR/dictionaries/fr.dic',
    'es': 'https://raw.githubusercontent.com/LibreOffice/dictionaries/master/es/es_ES.dic',
    'zh': 'https://raw.githubusercontent.com/fxsjy/jieba/master/jieba/dict.txt',
}
# jieba part-of-speech tags that mark ordinary vocabulary; proper-noun tags
# (nz, nt, ns, nr) are kept so 华为 or 深圳 survive the common-word filter.
ZH_COMMON_POS = {'n', 'v', 'a', 'd', 'vn', 'an', 't', 'f', 's', 'm', 'q', 'r', 'p', 'c', 'u', 'i', 'l'}

CACHE = os.path.join(tempfile.gettempdir(), 'docudis_lists')
SEP = '\x1f'
REFRESH = False


def sparql(query, retries=3):
    os.makedirs(CACHE, exist_ok=True)
    key = hashlib.sha1(query.encode('utf-8')).hexdigest()[:16]
    path = os.path.join(CACHE, f'sparql-{key}.json')
    if not REFRESH and os.path.exists(path):
        return json.load(open(path, encoding='utf-8'))
    for attempt in range(retries):
        r = requests.get(SPARQL, params={'query': query, 'format': 'json'},
                         headers={'User-Agent': UA}, timeout=300)
        if r.status_code == 200:
            rows = r.json()['results']['bindings']
            json.dump(rows, open(path, 'w', encoding='utf-8'), ensure_ascii=False)
            return rows
        print(f'  sparql {r.status_code}, retry {attempt + 1}', file=sys.stderr)
        time.sleep(10 * (attempt + 1))
    raise RuntimeError(f'SPARQL failed: {r.status_code} {r.text[:200]}')


def qid_of(row, key):
    return row[key]['value'].rsplit('/', 1)[1]


def clean(name):
    """Drops Wikidata's disambiguating tails: 福建省 (中华人民共和国), Total (company)."""
    return re.sub(r'\s*[（(][^()（）]*[)）]\s*$', '', name).strip()


def chunks(seq, n):
    for i in range(0, len(seq), n):
        yield seq[i:i + n]


def fetch_companies():
    """Two steps, because "business subclass x country" joined in one query
    times out on Wikidata: (1) every item of every business subclass with
    enough sitelinks, with its country, filtered to the markets client-side;
    (2) labels and aliases for those items in batches."""
    t = time.time()
    classes = [qid_of(r, 'c') for r in sparql('SELECT DISTINCT ?c WHERE { ?c wdt:P279* wd:%s }' % BUSINESS)]
    print(f'  {len(classes)} business subclasses, {time.time() - t:.0f}s')
    market_of = {c: m for m, cs in MARKETS.items() for c in cs}
    out = {}
    for i, chunk in enumerate(chunks(classes, 400)):
        t = time.time()
        rows = sparql(f"""
SELECT DISTINCT ?item ?sitelinks ?country WHERE {{
  VALUES ?class {{ {' '.join('wd:' + c for c in chunk)} }}
  ?item wdt:P31 ?class ; wikibase:sitelinks ?sitelinks ; wdt:P17 ?country .
  FILTER(?sitelinks >= {min(MIN_SITELINKS.values())})
}}""")
        n = 0
        for row in rows:
            market = market_of.get(qid_of(row, 'country'))
            sitelinks = int(row['sitelinks']['value'])
            if market is None or sitelinks < MIN_SITELINKS[market]:
                continue
            entry = out.setdefault(qid_of(row, 'item'), {'markets': set(), 'sitelinks': sitelinks, 'names': set()})
            entry['markets'].add(market)
            n += 1
        print(f'  classes chunk {i + 1}: {len(rows)} rows, {n} in target markets, {time.time() - t:.0f}s')
    ids = sorted(out)
    langs = ','.join(f'"{l}"' for l in LANGS)
    for i, chunk in enumerate(chunks(ids, 300)):
        rows = sparql(f"""
SELECT ?item (GROUP_CONCAT(DISTINCT ?name; separator="{SEP}") AS ?names) WHERE {{
  VALUES ?item {{ {' '.join('wd:' + q for q in chunk)} }}
  {{ ?item rdfs:label ?name }} UNION {{ ?item skos:altLabel ?name }}
  FILTER(LANG(?name) IN ({langs}))
}} GROUP BY ?item""")
        for row in rows:
            out[qid_of(row, 'item')]['names'].update(clean(n) for n in row['names']['value'].split(SEP) if n)
        if (i + 1) % 10 == 0:
            print(f'  names: {(i + 1) * 300} / {len(ids)} items')
    print(f'  companies: {len(out)} entities in target markets')
    return out


def fetch_places():
    out = {}
    zh = ','.join(f'"{l}"' for l in LANGS if l.startswith('zh'))
    for qid, (label, bare) in PLACE_CLASSES.items():
        rows = sparql(f"""
SELECT ?item (MAX(?pop) AS ?population) (GROUP_CONCAT(DISTINCT ?name; separator="{SEP}") AS ?names) WHERE {{
  ?item wdt:P31 wd:{qid} ; wdt:P17 wd:Q148 ; rdfs:label ?name .
  FILTER(LANG(?name) IN ({zh}))
  OPTIONAL {{ ?item wdt:P1082 ?pop }}
}} GROUP BY ?item""")
        for row in rows:
            population = int(float(row['population']['value'])) if 'population' in row else 0
            entry = out.setdefault(qid_of(row, 'item'), {'bare': bare, 'population': population, 'names': set()})
            entry['names'].update(clean(n) for n in row['names']['value'].split(SEP) if n and _is_cjk(n))
        print(f'  places {label} ({qid}): {len(rows)} rows')
    return out


def load_wordlist(lang):
    os.makedirs(CACHE, exist_ok=True)
    path = os.path.join(CACHE, f'{lang}.txt')
    if not os.path.exists(path):
        print(f'  downloading word list {lang} from {WORDLISTS[lang]}')
        r = requests.get(WORDLISTS[lang], headers={'User-Agent': UA}, timeout=120)
        r.raise_for_status()
        open(path, 'wb').write(r.content)
    words = set()
    for line in open(path, encoding='utf-8', errors='replace'):
        line = line.strip()
        if not line:
            continue
        if lang == 'zh':
            parts = line.split()
            if len(parts) >= 3 and parts[2] in ZH_COMMON_POS:
                words.add(parts[0])
        elif not line[0].isdigit():
            # hunspell entries keep their case: "apple/SM" is vocabulary,
            # "Airbus/M" a proper noun. Only lower-case entries count as
            # ordinary words.
            word = line.split('/')[0].split('\t')[0]
            if word == word.lower():
                words.add(word)
    return words


_cjk_re = re.compile('[㐀-鿿]')


def _is_cjk(s):
    return bool(_cjk_re.search(s))


# The app strips these before matching (BundledListDetector._edge).
EDGE = re.compile(r'^[\W_]+|[\W_]+$')

# Latin place names a company alias can collide with.
PLACE_WORDS = set("""
Alabama Alaska Arizona Arkansas California Colorado Connecticut Delaware Florida Georgia Hawaii
Idaho Illinois Indiana Iowa Kansas Kentucky Louisiana Maine Maryland Massachusetts Michigan
Minnesota Mississippi Missouri Montana Nebraska Nevada Ohio Oklahoma Oregon Pennsylvania
Tennessee Texas Utah Vermont Virginia Washington Wisconsin Wyoming
America Canada Mexico England Scotland Wales Ireland France Belgium Switzerland Spain Germany
Italy Portugal Netherlands Austria Australia China Japan India Brazil Argentina Chile Colombia
Peru Morocco Algeria Tunisia Senegal Europe Africa Asia
Canadá México Francia Bélgica Suiza España Alemania Italia Irlanda Inglaterra Escocia Europa
Allemagne Espagne Italie Belgique Suisse Angleterre Écosse Irlande Mexique Brésil Maroc
Algérie Tunisie Sénégal Chine Japon Inde Afrique
""".split())


def _fold(s):
    return ''.join(c for c in unicodedata.normalize('NFD', s) if unicodedata.category(c) != 'Mn').lower()


MAX_NAME = 120  # longer aliases are sentences, not names


def keep_name(n, sitelinks, common, place_set):
    if not n or ':' in n or '/' in n or len(n) > MAX_NAME:
        return False
    n = EDGE.sub('', n)  # judged as the app will match it
    if not n:
        return False
    if _is_cjk(n):
        if len(n) < 2 or n in common['zh'] or n in place_set:
            return False
        return len(n) >= 3 or sitelinks >= WELL_KNOWN
    if len(n) < 3 or not any(c.isalpha() for c in n) or n in place_set or n in PLACE_WORDS:
        return False
    if re.fullmatch(r'\S+\.[a-z]{2,}', n):
        return False  # domain name, the URL rule covers it
    if n == n.lower():
        return False  # pinyin and lower-case aliases (maidanglao); brands keep their case
    words = re.findall(r"[^\W\d_]+", n)
    if len(words) == 1:
        w = _fold(n)
        if w in common['en'] or w in common['fr'] or w in common['es']:
            return False
        if n.isupper() and len(n) <= 3:
            return False  # BMI, AMC, FRS: too many other meanings
    return True


def main():
    global REFRESH
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', default='data/lists')
    ap.add_argument('--stage', choices=['companies', 'places', 'all', 'refilter'], default='all')
    ap.add_argument('--refresh', action='store_true')
    args = ap.parse_args()
    REFRESH = args.refresh
    os.makedirs(args.out, exist_ok=True)

    common = {l: load_wordlist(l) for l in WORDLISTS}
    print('word lists:', {l: len(w) for l, w in common.items()})

    if args.stage == 'refilter':
        place_set = {n for p in json.load(open(os.path.join(args.out, 'places_zh.json'), encoding='utf-8'))
                     for n in p['names']}
        path = os.path.join(args.out, 'companies.json')
        kept, dropped = [], []
        for c in json.load(open(path, encoding='utf-8')):
            names = [n for n in c['names'] if keep_name(n, c['sitelinks'], common, place_set)]
            dropped += [n for n in c['names'] if n not in names]
            if names:
                kept.append({**c, 'names': names})
        json.dump(kept, open(path, 'w', encoding='utf-8', newline='\n'), ensure_ascii=False, indent=1)
        print(f'companies: {len(kept)} entities, {sum(len(c["names"]) for c in kept)} names, '
              f'{len(dropped)} names dropped by the filter: {sorted(dropped)[:60]}')
        return

    places = fetch_places()
    for p in places.values():
        names = set(p['names'])
        big = p['population'] >= BIG_CITY
        if p['bare'] == 'always' or (p['bare'] == 'big' and big):
            for n in p['names']:
                for suffix in ('市', '省', '自治区', '自治區', '特别行政区', '特別行政區'):
                    bare = n[:-len(suffix)]
                    if n.endswith(suffix) and len(bare) >= 2:
                        names.add(bare)
        elif p['bare'] == 'big':
            for n in p['names']:
                if n.endswith('市') and len(n) >= 4:
                    names.add(n[:-1])  # 齐齐哈尔, 呼和浩特: three characters or more are safe
        p['names'] = sorted(n for n in names if len(n) >= 2 and n not in common['zh'] and ':' not in n)
    places = {k: v for k, v in places.items() if v['names']}
    place_set = {n for p in places.values() for n in p['names']}
    if args.stage in ('places', 'all'):
        json.dump([{'id': k, 'names': v['names']} for k, v in sorted(places.items())],
                  open(os.path.join(args.out, 'places_zh.json'), 'w', encoding='utf-8', newline='\n'),
                  ensure_ascii=False, indent=1)
        print(f'places: {len(places)} entities, {len(place_set)} names')

    if args.stage in ('companies', 'all'):
        companies = fetch_companies()
        dropped = 0
        kept = []
        for qid, c in sorted(companies.items(), key=lambda kv: (-kv[1]['sitelinks'], kv[0])):
            names = sorted(n for n in c['names'] if keep_name(n, c['sitelinks'], common, place_set))
            dropped += len(c['names']) - len(names)
            if names:
                kept.append({'id': qid, 'markets': sorted(c['markets']), 'sitelinks': c['sitelinks'], 'names': names})
        json.dump(kept, open(os.path.join(args.out, 'companies.json'), 'w', encoding='utf-8', newline='\n'),
                  ensure_ascii=False, indent=1)
        by_market = {}
        for c in kept:
            for m in c['markets']:
                by_market[m] = by_market.get(m, 0) + 1
        print(f'companies: {len(kept)} entities, {sum(len(c["names"]) for c in kept)} names, {dropped} names dropped; by market {by_market}')


if __name__ == '__main__':
    main()
