// Sorting, filtering and saved choices for My library. Everything here is
// plain data in, plain data out: the whole library is fetched in one request
// and a few hundred books sort and filter instantly in the browser.

import { renderMarkdown } from '../markdown';
import type { LibraryBook } from '../types';
import { splitAuthors } from './authors';

export type SortKey = 'added' | 'title' | 'author' | 'year' | 'progress' | 'rating';
export type SortDir = 'asc' | 'desc';
export type ViewMode = 'grid' | 'list';
export type ReadStatus = 'unread' | 'reading' | 'finished';
/** The status filter: a reading status, or the want-to-read list. */
export type StatusFilter = ReadStatus | 'wanted';

export type Filters = {
	text: string;
	status: StatusFilter | null;
	/** Primary language code, as returned by primaryLanguage(). */
	language: string | null;
	/** A shelf id, or 'none' for books that sit on no shelf. */
	shelf: number | 'none' | null;
	/** An author's identity key (authors.ts), matching any of a book's authors. */
	author: string | null;
	/** Only books whose file the health check has remarks on. */
	file: 'issues' | null;
};

export type Prefs = { view: ViewMode; sort: SortKey; dir: SortDir };

export const SORT_KEYS: SortKey[] = ['added', 'title', 'author', 'year', 'progress', 'rating'];
export const STATUSES: StatusFilter[] = ['unread', 'reading', 'finished', 'wanted'];

/** The direction that makes sense when a sort key is first chosen. */
export function defaultDir(key: SortKey): SortDir {
	return key === 'title' || key === 'author' ? 'asc' : 'desc';
}

export function emptyFilters(): Filters {
	return { text: '', status: null, language: null, shelf: null, author: null, file: null };
}

export function activeFilterCount(f: Filters): number {
	return (f.status ? 1 : 0) + (f.language ? 1 : 0) + (f.shelf !== null ? 1 : 0) + (f.author ? 1 : 0) + (f.file ? 1 : 0);
}

// EPUBs carry their language as anything from "sv" to "sv-SE" to "swe".
const ISO3: Record<string, string> = {
	swe: 'sv',
	eng: 'en',
	fin: 'fi',
	nor: 'no',
	nob: 'nb',
	nno: 'nn',
	dan: 'da',
	isl: 'is',
	ice: 'is',
	deu: 'de',
	ger: 'de',
	fra: 'fr',
	fre: 'fr',
	spa: 'es',
	ita: 'it',
	por: 'pt',
	nld: 'nl',
	dut: 'nl',
	pol: 'pl',
	rus: 'ru',
	ell: 'el',
	gre: 'el',
	lat: 'la',
	epo: 'eo'
};

/** "sv-SE", "swe" and "SV" all become "sv"; unusable values become null. */
export function primaryLanguage(code: string | null | undefined): string | null {
	if (!code) return null;
	// A few books list several languages; the first is the main one.
	const first = code.split(/[,;]/)[0].trim().toLowerCase();
	const base = first.split(/[-_]/)[0];
	if (!/^[a-z]{2,3}$/.test(base)) return null;
	return ISO3[base] ?? base;
}

/** A book's language by name, e.g. "Swedish"; null when it has none that is usable. */
export function languageName(code: string | null | undefined, locale: string): string | null {
	const primary = primaryLanguage(code);
	return primary ? languageLabel(primary, locale) : null;
}

export function languageLabel(code: string, locale: string): string {
	try {
		const name = new Intl.DisplayNames([locale], { type: 'language' }).of(code);
		if (name && name !== code) return name.charAt(0).toUpperCase() + name.slice(1);
	} catch {
		// Unknown code or locale: fall through to the code itself.
	}
	return code;
}

/**
 * The year out of "2013-10-25T00:00:00+00:00", "2023-06" or Libris' "[2019]".
 * calibre's placeholder date 0101-01-01 counts as unknown.
 */
export function publishedYear(published: string | null | undefined): number | null {
	const m = published?.match(/^\D{0,2}(\d{4})/);
	if (!m) return null;
	const year = Number(m[1]);
	return year >= 1000 && year <= 2200 ? year : null;
}

/** The book's year: when the work first appeared, else this edition's year. */
export function bookYear(book: { first_published?: number | null; published?: string | null }): number | null {
	return book.first_published ?? publishedYear(book.published);
}

export function readStatus(book: LibraryBook): ReadStatus {
	const p = book.progress_percent;
	if (p == null) return 'unread';
	return p >= 0.99 ? 'finished' : 'reading';
}

/** "Sven Delblanc" sorts under D, "Lagerlöf, Selma" under L: the first author's surname. */
export function authorSortKey(author: string | null | undefined): string | null {
	return splitAuthors(author)[0]?.sort ?? null;
}

/** Lowercase without diacritics, so "lagerlof" finds "Lagerlöf". */
function fold(s: string): string {
	return s
		.normalize('NFD')
		.replace(/[̀-ͯ]/g, '')
		.toLowerCase();
}

export function filterBooks(books: LibraryBook[], f: Filters): LibraryBook[] {
	const words = fold(f.text).split(/\s+/).filter(Boolean);
	return books.filter((book) => {
		if (f.status === 'wanted' ? !book.want_to_read : f.status && readStatus(book) !== f.status) return false;
		if (f.language && primaryLanguage(book.language) !== f.language) return false;
		if (f.shelf === 'none' && book.shelf_ids.length > 0) return false;
		if (typeof f.shelf === 'number' && !book.shelf_ids.includes(f.shelf)) return false;
		if (f.author && !splitAuthors(book.author).some((p) => p.key === f.author)) return false;
		if (f.file === 'issues' && !(book.health_issues > 0)) return false;
		if (words.length > 0) {
			const hay = fold(`${book.title} ${book.author ?? ''} ${book.series ?? ''}`);
			if (!words.every((w) => hay.includes(w))) return false;
		}
		return true;
	});
}

export function sortBooks(
	books: LibraryBook[],
	key: SortKey,
	dir: SortDir,
	locale: string
): LibraryBook[] {
	const collator = new Intl.Collator(locale, { sensitivity: 'base', numeric: true });
	const sign = dir === 'asc' ? 1 : -1;
	const byTitle = (a: LibraryBook, b: LibraryBook) => collator.compare(a.title, b.title);

	// Books without a value go last whichever way the rest is ordered.
	function compare<T>(
		value: (book: LibraryBook) => T | null,
		order: (a: T, b: T) => number
	): (a: LibraryBook, b: LibraryBook) => number {
		return (a, b) => {
			const va = value(a);
			const vb = value(b);
			if (va === null && vb === null) return byTitle(a, b);
			if (va === null) return 1;
			if (vb === null) return -1;
			return sign * order(va, vb) || byTitle(a, b);
		};
	}

	const comparators: Record<SortKey, (a: LibraryBook, b: LibraryBook) => number> = {
		added: (a, b) => sign * (a.created_at.localeCompare(b.created_at) || a.id - b.id),
		title: (a, b) => sign * byTitle(a, b),
		author: compare(
			(book) => authorSortKey(book.author),
			(a, b) => collator.compare(a, b)
		),
		year: compare(
			(book) => bookYear(book),
			(a, b) => a - b
		),
		progress: compare(
			(book) => book.progress_percent ?? null,
			(a, b) => a - b
		),
		rating: compare(
			(book) => book.rating ?? null,
			(a, b) => a - b
		)
	};
	return [...books].sort(comparators[key]);
}

const snippets = new Map<string, string>();

/** The start of a description as plain text; descriptions arrive as HTML or Markdown. */
export function plainSnippet(description: string | null | undefined, max = 220): string {
	if (!description) return '';
	const cached = snippets.get(description);
	if (cached !== undefined) return cached;
	const doc = new DOMParser().parseFromString(renderMarkdown(description), 'text/html');
	const text = (doc.body.textContent ?? '').replace(/\s+/g, ' ').trim();
	const snippet = text.length > max ? `${text.slice(0, max).trimEnd()}…` : text;
	snippets.set(description, snippet);
	return snippet;
}

// View and sorting are a lasting preference per device; filters only need to
// survive opening a book and coming back.
const PREFS_KEY = 'legejo.library.prefs';
const FILTERS_KEY = 'legejo.library.filters';

export function loadPrefs(): Prefs {
	const prefs: Prefs = { view: 'grid', sort: 'added', dir: 'desc' };
	try {
		const saved = JSON.parse(localStorage.getItem(PREFS_KEY) ?? 'null');
		if (saved?.view === 'grid' || saved?.view === 'list') prefs.view = saved.view;
		if (SORT_KEYS.includes(saved?.sort)) prefs.sort = saved.sort;
		if (saved?.dir === 'asc' || saved?.dir === 'desc') prefs.dir = saved.dir;
	} catch {
		// No storage, or something else's data: the defaults stand.
	}
	return prefs;
}

export function savePrefs(prefs: Prefs) {
	try {
		localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));
	} catch {
		// Private windows and full disks: the choice just doesn't stick.
	}
}

/** The gallery/list choice alone, for pages that share it with the library. */
export function loadView(): ViewMode {
	return loadPrefs().view;
}

export function saveView(view: ViewMode) {
	savePrefs({ ...loadPrefs(), view });
}

export function loadFilters(): Filters {
	const filters = emptyFilters();
	try {
		const saved = JSON.parse(sessionStorage.getItem(FILTERS_KEY) ?? 'null');
		if (typeof saved?.text === 'string') filters.text = saved.text;
		if (STATUSES.includes(saved?.status)) filters.status = saved.status;
		if (typeof saved?.language === 'string') filters.language = saved.language;
		if (saved?.shelf === 'none' || typeof saved?.shelf === 'number') filters.shelf = saved.shelf;
		if (typeof saved?.author === 'string') filters.author = saved.author;
		if (saved?.file === 'issues') filters.file = saved.file;
	} catch {
		// As above.
	}
	return filters;
}

export function saveFilters(filters: Filters) {
	try {
		sessionStorage.setItem(FILTERS_KEY, JSON.stringify(filters));
	} catch {
		// As above.
	}
}
