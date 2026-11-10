// A book's vocabulary as the server counted it (GET /api/books/:id/vocabulary),
// and the reader's known words (PUT /api/vocabulary/known).

export type StudyRow = {
	stem: string;
	word: string;
	forms: string[];
	count: number;
	name: boolean;
	known: boolean;
	english: string | null;
	found_as: string | null;
	class: WordClass | null;
};

export type WordClass = 'verb' | 'noun' | 'adj' | 'other';
export const WORD_CLASSES: WordClass[] = ['verb', 'noun', 'adj', 'other'];

export type Vocab = {
	language: string;
	words: number;
	stems: number;
	hapax: number;
	coverage: Record<string, number>;
	curve: [number, number][];
	books_in_language: number;
	rated_from: number;
	difficulty: { level: number; compared_with: number } | null;
	short: boolean;
	characteristic: string[];
	common: string[];
	distinctive_from: number;
	names: string[];
	study: StudyRow[] | null;
	known_share: number | null;
	study_source: string | null;
};

// A book's page asks for its vocabulary from more than one place at once;
// requests in flight are shared, and nothing is kept once they settle.
const inFlight = new Map<number, Promise<Vocab | null>>();

export function loadVocab(id: number, fetcher: typeof fetch = fetch): Promise<Vocab | null> {
	const pending = inFlight.get(id);
	if (pending) return pending;
	const request: Promise<Vocab | null> = (async () => {
		try {
			const res = await fetcher(`/api/books/${id}/vocabulary`);
			return res.ok ? await res.json() : null;
		} catch {
			return null;
		}
	})().finally(() => inFlight.delete(id));
	inFlight.set(id, request);
	return request;
}

/** A selected word as a book's study list has it (GET /api/books/:id/vocabulary/word). */
export type WordInBook = {
	stem: string;
	word: string;
	english: string | null;
	class: WordClass | null;
	known: boolean;
	count: number;
	/** The language's words have tables of forms (GET /api/analysis/forms). */
	forms?: boolean;
};

/** The word in the book's study list, or null when it is not there. */
export async function loadWordInBook(bookId: number, word: string, fetcher: typeof fetch = fetch): Promise<WordInBook | null> {
	try {
		const res = await fetcher(`/api/books/${bookId}/vocabulary/word?w=${encodeURIComponent(word)}`);
		return res.ok ? await res.json() : null;
	} catch {
		return null;
	}
}

/** Mark a word known or not; false when the server did not take it. */
export async function setKnown(language: string, stem: string, known: boolean): Promise<boolean> {
	try {
		const res = await fetch('/api/vocabulary/known', {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ language, stem, known })
		});
		return res.ok;
	} catch {
		return false;
	}
}

/** Tick or untick many words at once (all the words above one in a list). */
export async function setKnownMany(language: string, stems: string[], known: boolean): Promise<boolean> {
	try {
		const res = await fetch('/api/vocabulary/known/many', {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ language, stems, known })
		});
		return res.ok;
	} catch {
		return false;
	}
}

/** Share of the book's words there are to learn (names left out) that the known words cover. */
export function knownCover(v: Vocab, known: Set<string>): number {
	if (!v.study) return 0;
	let covered = 0;
	let names = 0;
	for (const w of v.study) {
		if (w.name) names += w.count;
		else if (known.has(w.stem)) covered += w.count;
	}
	return covered / Math.max(1, v.words - names);
}

/** The language's name as it stands inside a sentence ("svenska", "Swedish"). */
export function languageInSentence(code: string, locale: string): string {
	try {
		return new Intl.DisplayNames([locale], { type: 'language' }).of(code) ?? code;
	} catch {
		return code;
	}
}

export const fmt = (n: number, locale: string) => n.toLocaleString(locale);
export const percent = (x: number) => Math.round(x * 100);
/** For CSS: no space before the sign. */
export const css = (x: number) => `${(x * 100).toFixed(1)}%`;

/** Rows added to a paged list, leaving out those it already has (a list can shift between pages). */
export function appendUnique<T>(rows: T[], more: T[], key: (r: T) => string | number): T[] {
	const seen = new Set(rows.map(key));
	return [...rows, ...more.filter((r) => !seen.has(key(r)))];
}

/** For each word of a list, how many words from the top down to and including it are not known. */
export function unknownPrefix(words: { stem: string }[], known: Set<string>): Map<string, number> {
	const upTo = new Map<string, number>();
	let unknown = 0;
	for (const w of words) {
		if (!known.has(w.stem)) unknown += 1;
		upTo.set(w.stem, unknown);
	}
	return upTo;
}

// The words page (GET /api/vocabulary/languages and /overview).
export type WordLanguage = { language: string; books: number; known: number };
export type WordRow = {
	stem: string;
	word: string;
	english: string | null;
	class: WordClass | null;
	known: boolean;
	/** How many of the reader's books have the word, and up to twenty of them (those that use it most). */
	books: number;
	book_ids: number[];
	count: number;
};
export type BookShare = { id: number; title: string; author: string | null; share: number; reading: boolean };
export type Overview = {
	language: string;
	/** The first of the reader's books: those being read, then those known most of. */
	books: BookShare[];
	books_total: number;
	next: WordRow[];
	/** How many words the reader knows; the words come from `loadKnownWords`. */
	known: number;
	/** How many of the commonest words in the reader's books they know. */
	common: { known: number; of: number };
	source: string | null;
};

export type KnownSort = 'recent' | 'common' | 'alpha';
export type KnownQuery = { q?: string; sort?: KnownSort; class?: WordClass | 'all'; offset?: number; limit?: number };

/** The words the reader knows in a language, a page at a time (all without a limit). */
export async function loadKnownWords(
	language: string,
	query: KnownQuery = {},
	fetcher: typeof fetch = fetch
): Promise<{ total: number; words: WordRow[] } | null> {
	const params = new URLSearchParams({ language });
	if (query.q) params.set('q', query.q);
	if (query.sort) params.set('sort', query.sort);
	if (query.class && query.class !== 'all') params.set('class', query.class);
	if (query.offset) params.set('offset', String(query.offset));
	if (query.limit) params.set('limit', String(query.limit));
	try {
		const res = await fetcher(`/api/vocabulary/known/words?${params}`);
		return res.ok ? await res.json() : null;
	} catch {
		return null;
	}
}

export async function loadLanguages(fetcher: typeof fetch = fetch): Promise<WordLanguage[]> {
	try {
		const res = await fetcher('/api/vocabulary/languages');
		return res.ok ? await res.json() : [];
	} catch {
		return [];
	}
}

export async function loadOverview(language: string, fetcher: typeof fetch = fetch): Promise<Overview | null> {
	try {
		const res = await fetcher(`/api/vocabulary/overview?${new URLSearchParams({ language })}`);
		return res.ok ? await res.json() : null;
	} catch {
		return null;
	}
}

/** The words of the reader's books in a language that match what they typed. */
export async function loadSearch(language: string, q: string, fetcher: typeof fetch = fetch): Promise<WordRow[] | null> {
	try {
		const res = await fetcher(`/api/vocabulary/search?${new URLSearchParams({ language, q })}`);
		return res.ok ? (await res.json()).matches : null;
	} catch {
		return null;
	}
}

// Word lists as files: plain text, or tab-separated for Anki's text import
// with the headers Anki reads itself.
export type ExportRow = { word: string; forms?: string[]; english: string | null };

export function download(name: string, text: string) {
	const url = URL.createObjectURL(new Blob([text], { type: 'text/plain;charset=utf-8' }));
	const a = Object.assign(document.createElement('a'), { href: url, download: name });
	a.click();
	setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function asText(heading: string[], rows: ExportRow[]): string {
	const label = (w: ExportRow) => w.word + (w.forms?.length ? ` (${w.forms.join(', ')})` : '');
	const width = Math.max(0, ...rows.map((w) => label(w).length));
	const lines = rows.map((w) => `${label(w).padEnd(width)}   ${w.english ?? ''}`.trimEnd());
	return [...heading, '', ...lines, ''].join('\n');
}

/** A deck under Legejo's: Anki nests decks on "::", so a name must not hold one. */
export const ankiDeck = (name: string) => `Legejo::${name.replace(/::/g, ':')}`;

export function asAnki(deck: string, tags: string, rows: ExportRow[], about?: [string, string]): string {
	const esc = (x: string) => x.replace(/[\t\n]/g, ' ').replace(/&/g, '&amp;').replace(/</g, '&lt;');
	const lines = rows.map((w) => {
		const forms = w.forms?.length ? `<br><small>${esc(w.forms.join(' · '))}</small>` : '';
		return [esc(w.word), esc(w.english ?? '') + forms, tags].join('\t');
	});
	if (about) lines.push([esc(about[0]), esc(about[1]), 'legejo'].join('\t'));
	const head = ['#separator:tab', '#html:true', '#notetype:Basic', `#deck:${deck}`, '#tags column:3'];
	return [...head, ...lines, ''].join('\n');
}

/** A sentence with a word from one of the reader's books, the word marked. */
export type Example = { book: number; title: string; pieces: { text: string; hit: boolean }[] };

/** A few example sentences with a word from the reader's books in a language. */
export async function loadExamples(language: string, stem: string, fetcher: typeof fetch = fetch): Promise<Example[] | null> {
	try {
		const params = new URLSearchParams({ language, stem });
		const res = await fetcher(`/api/vocabulary/examples?${params}`);
		return res.ok ? (await res.json()).examples : null;
	} catch {
		return null;
	}
}

/**
 * What a share of a book's running words known means for reading it, after
 * the research on lexical coverage (Hu & Nation 2000; Laufer &
 * Ravenhorst-Kalovski 2010): some 95 % to follow a text with little help,
 * 98 % to read it with ease.
 */
export type ReadingLevel = 'hard' | 'tough' | 'support' | 'good' | 'easy';
export function readingLevel(share: number): ReadingLevel {
	if (share >= 0.98) return 'easy';
	if (share >= 0.95) return 'good';
	if (share >= 0.9) return 'support';
	if (share >= 0.8) return 'tough';
	return 'hard';
}

/** The level that goes with the percentage shown, so that 94.9 % rounded to 95 reads as 95. */
export function shownLevel(share: number): ReadingLevel {
	return readingLevel(percent(share) / 100);
}

/** The reader's books in a language with how much of each they know, a page at a time. */
export async function loadBooks(
	language: string,
	offset: number,
	limit: number,
	fetcher: typeof fetch = fetch
): Promise<{ total: number; books: BookShare[] } | null> {
	try {
		const params = new URLSearchParams({ language, offset: String(offset), limit: String(limit) });
		const res = await fetcher(`/api/vocabulary/books?${params}`);
		return res.ok ? await res.json() : null;
	} catch {
		return null;
	}
}

/** A level test: words from each frequency band of the reader's books. */
export type LevelBand = { from: number; to: number; words: { stem: string; word: string; english: string | null }[] };
export async function loadLevelTest(language: string, fetcher: typeof fetch = fetch): Promise<{ words: number; bands: LevelBand[] } | null> {
	try {
		const res = await fetcher(`/api/vocabulary/level?${new URLSearchParams({ language })}`);
		return res.ok ? await res.json() : null;
	} catch {
		return null;
	}
}

/** Send a level test's answers; the server ticks the words it takes the reader to know. */
export async function sendLevelTest(
	language: string,
	answers: { stem: string; known: boolean }[]
): Promise<{ estimate: number; ticked: number } | null> {
	try {
		const res = await fetch('/api/vocabulary/level', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ language, answers })
		});
		return res.ok ? await res.json() : null;
	} catch {
		return null;
	}
}
