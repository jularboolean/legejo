// Authors as people, out of the free-text author field.
//
// EPUBs and Libris write authors in every form: "Selma Lagerlöf",
// "Lagerlöf, Selma", "Lagerlöf, Selma, 1858-1940", "Maj Sjöwall, Per Wahlöö"
// (our upload joins several dc:creator with ", "), "Sjöwall & Wahlöö".
// splitAuthors() turns each into separate people with a display name
// ("Selma Lagerlöf"), a sort key ("Lagerlöf Selma") and an identity key
// (folded display name), so the same person written two ways counts once.

import type { LibraryBook } from '../types';

export type Person = {
	/** "Selma Lagerlöf" */
	name: string;
	/** "Lagerlöf Selma", for ordering by surname. */
	sort: string;
	/** Lowercase, no diacritics, no punctuation: the identity. */
	key: string;
};

export function fold(s: string): string {
	return s
		.normalize('NFD')
		.replace(/[̀-ͯ]/g, '')
		.toLowerCase();
}

function keyOf(name: string): string {
	return fold(name)
		.replace(/[^\p{L}\p{N}]+/gu, ' ')
		.trim();
}

/** Dates and roles that Libris and others append: "1858-1940", "f. 1946", "(red.)". */
function isNoise(segment: string): boolean {
	const s = segment.trim();
	return (
		s === '' ||
		/\d{3,4}/.test(s) ||
		/^\(.*\)$/.test(s) ||
		/^(red|ed|eds|övers|transl|ill|förf|author|editor)\.?$/i.test(s)
	);
}

function person(first: string, surname: string): Person {
	const name = `${first} ${surname}`.replace(/\s+/g, ' ').trim();
	const sort = `${surname} ${first}`.replace(/\s+/g, ' ').trim();
	return { name, sort, key: keyOf(name) };
}

/** Words that belong to the surname: "Le Guin", "van Gogh", "af Klint", "de la Motte". */
const PARTICLES = new Set(['le', 'la', 'de', 'del', 'della', 'der', 'den', 'des', 'di', 'da', 'du', 'van', 'von', 'af', 'av', 'ten', 'ter', 'zu', 'dos', 'das', 'st.', 'mac', 'o']);

/** One name in natural order: the last word is the surname, with any particles before it. */
function natural(raw: string): Person | null {
	const words = raw.trim().split(/\s+/).filter(Boolean);
	if (words.length === 0) return null;
	const surname = [words.pop() as string];
	while (words.length > 1 && PARTICLES.has(words[words.length - 1].toLowerCase())) {
		surname.unshift(words.pop() as string);
	}
	return person(words.join(' '), surname.join(' '));
}

function fromCommas(part: string): Person[] {
	const segments = part
		.split(',')
		.map((s) => s.trim())
		.filter((s) => !isNoise(s));
	if (segments.length === 0) return [];
	if (segments.length === 1) {
		const p = natural(segments[0]);
		return p ? [p] : [];
	}
	const oneWord = (s: string) => !/\s/.test(s);
	// A surname may carry particles: "Le Guin", "van Eyck".
	const surnameLike = (s: string) => {
		const words = s.split(/\s+/);
		return words.slice(0, -1).every((w) => PARTICLES.has(w.toLowerCase()));
	};
	// "Lagerlöf, Selma" (and "Lagerlöf, Selma Ottilia", "Le Guin, Ursula K."): surname first.
	if (segments.length === 2 && surnameLike(segments[0])) {
		return [person(segments[1], segments[0])];
	}
	// "Strindberg, August, Lagerlöf, Selma": surname/first pairs.
	if (segments.length % 2 === 0 && segments.every(oneWord)) {
		const out: Person[] = [];
		for (let i = 0; i < segments.length; i += 2) out.push(person(segments[i + 1], segments[i]));
		return out;
	}
	// "Maj Sjöwall, Per Wahlöö": several names in natural order.
	return segments.map(natural).filter((p): p is Person => p !== null);
}

const cache = new Map<string, Person[]>();

/** The people in an author field, in the order written, without duplicates. */
export function splitAuthors(author: string | null | undefined): Person[] {
	const raw = author?.trim();
	if (!raw) return [];
	const cached = cache.get(raw);
	if (cached) return cached;
	const parts = raw.split(/\s*(?:;|&|\s+och\s+|\s+and\s+|\s+und\s+|\s+et\s+|\s+y\s+)\s*/i);
	const seen = new Set<string>();
	const people: Person[] = [];
	for (const p of parts.flatMap(fromCommas)) {
		if (p.key && !seen.has(p.key)) {
			seen.add(p.key);
			people.push(p);
		}
	}
	cache.set(raw, people);
	return people;
}

export type AuthorEntry = Person & {
	books: LibraryBook[];
	finished: number;
};

/**
 * Every author in the library with their books. When the same person is
 * written several ways, the most common spelling is shown.
 */
export function collectAuthors(books: LibraryBook[], locale: string): AuthorEntry[] {
	const byKey = new Map<string, { spellings: Map<string, number>; person: Person; books: LibraryBook[] }>();
	for (const book of books) {
		for (const p of splitAuthors(book.author)) {
			let entry = byKey.get(p.key);
			if (!entry) {
				entry = { spellings: new Map(), person: p, books: [] };
				byKey.set(p.key, entry);
			}
			entry.spellings.set(p.name, (entry.spellings.get(p.name) ?? 0) + 1);
			entry.books.push(book);
		}
	}
	const collator = new Intl.Collator(locale, { sensitivity: 'base', numeric: true });
	return [...byKey.values()]
		.map(({ spellings, person: p, books: list }) => {
			const name = [...spellings.entries()].sort((a, b) => b[1] - a[1])[0][0];
			const best = splitAuthors(name)[0] ?? p;
			const finished = list.filter((b) => (b.progress_percent ?? 0) >= 0.99).length;
			return { ...best, key: p.key, books: list, finished };
		})
		.sort((a, b) => collator.compare(a.sort, b.sort));
}

/** The letter an author is filed under: "Ö" for Öberg, "#" for digits. */
export function initial(sort: string, locale: string): string {
	const c = sort.trim().charAt(0).toLocaleUpperCase(locale);
	return /\p{L}/u.test(c) ? c : '#';
}
