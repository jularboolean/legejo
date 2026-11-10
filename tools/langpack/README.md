# Language packs

A language pack gives books in a language a study list: the words of a book
in order of how often they occur, each taken to its dictionary form (*hizo*
to *hacer*) with a short English meaning. Esperanto is the one language
built into Legejo: it comes from ESPDIC and cannot be replaced. Every other
language needs a pack. Ready-made packs are listed at
<https://legejo.se/languages>, and an admin installs one with a click under
Admin → Languages. A pack for any other language is made from Wiktionary with
one command and imported from a zip file or put in the data directory.

## Build one

[kaikki.org](https://kaikki.org/dictionary/) publishes the English
Wiktionary's entries per language, with meanings in English and every
inflected form. Download the language's file and run the `langpack` program
that ships in the Legejo image. For Swahili:

```sh
curl -O https://kaikki.org/dictionary/Swahili/kaikki.org-dictionary-Swahili.jsonl.gz
docker run --rm -v "$PWD:/work" slurvdjur/legejo langpack sw Kiswahili /work/kaikki.org-dictionary-Swahili.jsonl.gz /work/sw
```

The arguments: the language code Legejo files the books under (see
[Codes](#codes)), the language's name as its speakers write it (shown in the
app), the downloaded file, and the directory to write the pack into. On
Linux add `--user "$(id -u):$(id -g)"` before `slurvdjur/legejo` so that the
files are yours. From a source checkout the same is

```sh
cd server && cargo run --release --bin langpack -- sw Kiswahili ../kaikki.org-dictionary-Swahili.jsonl.gz ../sw
```

It takes a few seconds and prints a summary: how many lemmas (dictionary
words) and forms were found and how many forms can be more than one lemma;
what was skipped; and the little words (prepositions, pronouns, adverbs) with
most senses, each with its meaning as the pack has it. Those few lines are
the quickest check that the glosses read well. `0 lemmas without a gloss`
and `0 with wikitext left in it` are what you want to see.

Wiktionary's content is licensed CC BY-SA 4.0, and so is a pack made from it.
`pack.json` carries the attribution; keep it.

## Extras for the analysis

A book's Analysis page can count the forms a book is written in (so many
subjunctives, so many preterites), show the table of a word's forms in the
reader, and estimate the book's level. That takes two more files, which the
builder writes when asked:

```sh
langpack es Español kaikki.org-dictionary-Spanish.jsonl.gz es --tags \
  --frequency words.txt "Where the list comes from, and its licence"
```

- `--tags` writes `tags.tsv`: what the inflection tables of Wiktionary say
  each form is.
- `--frequency <list> "<source>"` writes `frequency.tsv`: the pack's lemmas
  in order of frequency, from a list of words as written, most common first,
  one per line with or without its count (`de 123456`). Wiktionary has no
  frequency data, so the list comes from elsewhere; the second argument says
  from where and under which licence, and is shown wherever the estimate is.
  Mind the list's licence: the pack is distributed with it.
- Either writes `extras.json`, which says what there is.

The extras sit in the pack's directory beside the other files, and a zip
with them imports as any pack does. Legejo 1.46 and older refuse a zip that
holds them, which is why the catalogue keeps them in a zip of their own
(`<code>.extras.zip`, named under `extras` in `index.json`) that only newer
servers ask for.

## Install

From the catalogue: Admin → Languages lists the packs at legejo.se with an
Install button. The server downloads `<code>.zip`, checks it against the
sha256 in the catalogue and installs it. `LEGEJO_LANGUAGES_URL` points the
server at another catalogue (an `index.json` and the zips next to it), or,
set to the empty string, turns the catalogue off.

From a file: zip `pack.json`, `lemmas.tsv` and `forms.tsv` (and optionally a
`README.txt`) and import the zip under Admin → Languages → Import from a
file. A pack for a language already there replaces it.

By hand: copy the directory into the data directory under `languages/<code>/`
(with Docker, `/data/languages/sw/`):

```
/data/languages/sw/pack.json
/data/languages/sw/lemmas.tsv
/data/languages/sw/forms.tsv
```

and restart Legejo. Books in that language are counted again in the
background and get a study list.

## Codes

A code is two or three lower-case letters: the ISO 639-1 code when the
language has one (`sw`), else the ISO 639-3 code (`yor`). Books name their
language in their EPUB metadata in several ways (`sw`, `swa`, `Swahili`,
`Kiswahili`); the `also` list in `pack.json` holds the other names that
should reach the pack. The builder fills it in for the languages it knows,
and always adds the dump's English name and the name you gave. Add more by
hand if your books use another.

## Improve a pack by hand

A pack is three text files, UTF-8, tab-separated, which any editor opens:

- `pack.json`: the language, its name, version, source, licence and
  attribution, and the `also` list.
- `lemmas.tsv`: one lemma per line: the word in lower case (the key), its
  parts of speech separated by commas (Wiktionary's names: `verb`, `noun`,
  `adj`, `adv`, `prep`, `pron`, `det`, `article`, `conj`, `particle`, `num`,
  `intj` …), its English, and, only when the word is written otherwise
  than its key, the word as written (`haus … Haus` for German nouns). The
  lines are sorted by key in byte order; Legejo sorts them itself when it
  loads a text pack, so an edit out of order does no harm.
- `forms.tsv`: one form per line: the form in lower case, then the key or
  keys (comma-separated) of the lemmas it can be. The lemma itself is a form
  too (`hacer hacer`). Any order; a repeated pair is harmless.

And the extras, when the pack has them:

- `tags.tsv`: one form of a lemma per line: the form, the lemma's key, and
  what the form is, as tags separated by spaces (`hizo hacer third-person
  singular preterite indicative`); a form that can be several things has
  them separated by semicolons. The tags are Wiktionary's: `present`,
  `preterite`, `imperfect`, `past`, `future`, `perfect`, `pluperfect`,
  `indicative`, `subjunctive`, `conditional`, `imperative`, `infinitive`,
  `gerund`, `participle`, the persons, `singular`, `plural`, the genders,
  the cases, `comparative`, `superlative`, `definite`, `indefinite`.
- `frequency.tsv`: the lemmas' keys in order of frequency, one per line.
- `extras.json`: `language`, `version`, and `frequency`, the line about the
  list's source.

Forms point at keys, not at line numbers, so lines in `lemmas.tsv` can be
edited, added and removed freely. To fix a meaning, edit the third column.
To add a word, add its line to `lemmas.tsv` and its forms to `forms.tsv`. To
stop a form from going to the wrong lemma, change its line in `forms.tsv`.
A form that is not in `forms.tsv` is not counted; a lemma that no form
points at is never shown. Restart Legejo to pick the change up.

## Limitations

- Separable verbs are not joined: German *kommt an* is counted as *kommen*
  and *an*; *ankommen* is found only when written in one word.
- Wiktionary has no frequency data. The study list orders words by their
  occurrences in the book, and the summary orders little words by their
  number of senses. A book's estimated level needs a frequency list from
  elsewhere (see Extras).
- `tags.tsv` holds what the inflection tables say. A form that Wiktionary
  describes only in an entry of its own ("plural of …") has no tags, and
  the lemma itself seldom has: the grammar counts verb forms and cases
  well, number and gender not at all.
- Packs are read from the local data directory.
- A form that can be several lemmas (*fue*: *ser* or *ir*) is listed with
  all of them; Legejo picks when it counts.

## For developers

The Spanish, French and German packs the server's tests use
(`server/tests/packs/`) are built the same way, with `--index`, which writes
the pre-built look-up index `forms.fst` and `ambiguous.tsv` instead of
`forms.tsv`, so that nothing is built when a test loads them:

```sh
cd server
cargo run --release --bin langpack -- es Español kaikki.org-dictionary-Spanish.jsonl.gz tests/packs/es --index
cargo run --release --bin langpack -- fr Français kaikki.org-dictionary-French.jsonl.gz tests/packs/fr --index
cargo run --release --bin langpack -- de Deutsch kaikki.org-dictionary-German.jsonl.gz tests/packs/de --index
```

`forms.fst` maps each form to a lemma's line number in `lemmas.tsv`, or,
with bit 40 set, to a line of `ambiguous.tsv` listing several. The
extraction rules live in `server/src/bin/langpack.rs`, each a small
function with a test on the fixture at the end of the file
(`cargo test --bin langpack`); `src/langpack.rs` reads the packs.
