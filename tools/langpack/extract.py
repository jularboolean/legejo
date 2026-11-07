#!/usr/bin/env python3
"""Make the text half of a Legejo language pack from kaikki.org's extract of
the English Wiktionary (https://kaikki.org/dictionary/<Language>/), which
gives each word of the language with English meanings and its inflected
forms.

    curl -O https://kaikki.org/dictionary/Spanish/kaikki.org-dictionary-Spanish.jsonl.gz
    python3 tools/langpack/extract.py es "Español" kaikki.org-dictionary-Spanish.jsonl.gz out/es

writes out/es/lemmas.tsv (lemma, parts of speech, English), out/es/forms.tsv
(form, lemmas) and out/es/pack.json. Then `cargo run --bin langpack out/es`
turns forms.tsv into forms.fst and the pack is complete; see README.md.
"""
import gzip, json, re, sys
from collections import defaultdict
from datetime import date
from pathlib import Path

code, name, src, out = sys.argv[1], sys.argv[2], sys.argv[3], Path(sys.argv[4])
out.mkdir(parents=True, exist_ok=True)

SKIP_POS = {'name', 'proverb', 'phrase', 'prefix', 'suffix', 'infix', 'interfix', 'character', 'symbol', 'abbrev', 'romanization'}
BAD_FORM_TAGS = {'table-tags', 'inflection-template', 'romanization', 'class', 'auxiliary', 'obsolete', 'archaic'}
# Parts of speech of little words: shown first when a word is one of them too
# ("a" the preposition before "a" the letter).
LITTLE = ['article', 'prep', 'conj', 'pron', 'det', 'adv', 'particle', 'contraction', 'num', 'intj']

forms = defaultdict(set)                     # form -> lemmas
meanings = defaultdict(dict)                 # lemma -> pos -> [glosses]


def short(g):
    return re.sub(r'\s*\([^)]*\)', '', g).strip().rstrip('.').strip()


with gzip.open(src, 'rt', encoding='utf-8') as f:
    for line in f:
        e = json.loads(line)
        word, pos = e.get('word', ''), e.get('pos', '')
        if not word or ' ' in word or pos in SKIP_POS:
            continue
        lw = word.lower()
        glosses, fallback = [], []
        for s in e.get('senses', []):
            if s.get('form_of'):
                for x in s['form_of']:
                    lemma = x.get('word', '').lower()
                    if lemma and ' ' not in lemma:
                        forms[lw].add(lemma)
                continue
            gl = s.get('glosses') or []
            if not gl:
                continue
            g = short(gl[-1] if len(gl) > 1 else gl[0])
            if not g or g.lower().startswith('the name of the') or 'letter' in s.get('tags', []):
                continue
            (fallback if {'obsolete', 'archaic', 'rare'} & set(s.get('tags', [])) else glosses).append(g)
        glosses = glosses or fallback
        if not glosses:
            continue
        meanings[lw].setdefault(pos, [])
        for g in glosses:
            if g not in meanings[lw][pos]:
                meanings[lw][pos].append(g)
        forms[lw].add(lw)
        for fm in e.get('forms', []):
            form = fm.get('form', '').lower()
            if form and ' ' not in form and form != '-' and not BAD_FORM_TAGS & set(fm.get('tags', [])):
                forms[form].add(lw)

lemmas = sorted(meanings)
index = {l: i for i, l in enumerate(lemmas)}
with open(out / 'lemmas.tsv', 'w', encoding='utf-8') as w:
    for l in lemmas:
        given = list(meanings[l])
        poss = sorted(given, key=lambda p: LITTLE.index(p) if p in LITTLE else len(LITTLE) + given.index(p))
        parts = ['; '.join(meanings[l][p][:3]) for p in poss[:2]]
        w.write(f"{l}\t{','.join(poss)}\t{' / '.join(parts)[:110]}\n")
kept = 0
with open(out / 'forms.tsv', 'w', encoding='utf-8') as w:
    # Sorted by code point, which is the byte order of UTF-8 that an fst needs.
    for form in sorted(forms):
        ls = sorted({index[l] for l in forms[form] if l in index})
        if ls:
            w.write(form + '\t' + ','.join(map(str, ls)) + '\n')
            kept += 1
(out / 'pack.json').write_text(json.dumps({
    'format': 1,
    'language': code,
    'name': name,
    'version': date.today().isoformat(),
    'source': 'Wiktionary, extracted by kaikki.org',
    'license': 'CC BY-SA 4.0',
    'attribution': 'Wiktionary contributors (CC BY-SA 4.0), via kaikki.org',
}, ensure_ascii=False, indent=2) + '\n')
print(f'{code}: {len(lemmas)} lemmas, {kept} forms')
