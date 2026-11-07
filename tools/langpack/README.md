# Language packs

A language pack gives books in a language a study list: their words in order
of frequency, each taken to its dictionary form (*hizo* to *hacer*) with a
short English meaning. Legejo has Spanish built in, and Esperanto through
ESPDIC. A pack for any other language can be made from Wiktionary and put in
the data directory; Legejo reads it the next time it starts.

## Making one

[kaikki.org](https://kaikki.org/dictionary/) publishes the English
Wiktionary's entries per language, with meanings in English and every
inflected form. For Swahili:

```sh
curl -O https://kaikki.org/dictionary/Swahili/kaikki.org-dictionary-Swahili.jsonl.gz
python3 tools/langpack/extract.py sw "Kiswahili" kaikki.org-dictionary-Swahili.jsonl.gz out/sw
cd server && cargo run --release --bin langpack ../out/sw
```

The first argument is the language's ISO 639-1 code, as Legejo catalogues
books (see the language list in the edit form).

## Using one

Copy the pack's directory into the data directory under `languages/`:

```
/data/languages/sw/pack.json
/data/languages/sw/forms.fst
/data/languages/sw/ambiguous.tsv
/data/languages/sw/lemmas.tsv
```

and restart Legejo. Books in Swahili are counted again with the pack and get
a study list. A pack here wins over a built-in one for the same language, so
an operator can also bring a newer Spanish pack.

## What is in a pack

- `pack.json`: `format` (1), `language`, `name`, `version`, `source`,
  `license` and `attribution`, shown with the meanings.
- `lemmas.tsv`: one lemma per line, sorted by code point: the lemma, its parts
  of speech (comma-separated, Wiktionary's names: `verb`, `noun`, `adj`,
  `prep` …) and its meaning. Lines are numbered from 0.
- `forms.fst`: an [fst](https://docs.rs/fst) map from every lower-case form
  to a lemma's line number, or, with bit 40 set, to a line of
  `ambiguous.tsv`.
- `ambiguous.tsv`: per line, the line numbers of the lemmas a form can be.

Data from Wiktionary is under CC BY-SA 4.0, and so is a pack made from it;
keep the attribution in `pack.json`.
