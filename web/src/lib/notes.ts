// The list of all one's highlights and notes: in order as they stand in
// each book, books with the latest change first, and the search over them.

import { fold } from './library/authors';

export type Note = {
	id: number;
	book_id: number;
	source: 'web' | 'kobo' | string;
	cfi: string | null;
	text: string;
	note: string | null;
	color: string | null;
	created_at: string;
	updated_at: string;
	book_title: string;
	book_author: string | null;
	book_has_cover: boolean;
	book_changed_at: string;
};

export type BookNotes = { bookId: number; title: string; author: string | null; hasCover: boolean; changedAt: string; notes: Note[] };

/**
 * Where a CFI starts, as numbers to compare: its steps and offsets, the
 * range's own start joined to the path they share. "epubcfi(/6/4[c]!/4/2,/1:0,/1:9)"
 * starts at 6, 4, 4, 2, 1, 0.
 */
function startOf(cfi: string): number[] {
	const inner = cfi.trim().replace(/^epubcfi\(/, '').replace(/\)$/, '');
	const [common, start = ''] = inner.split(',');
	return (common + start).match(/\d+/g)?.map(Number) ?? [];
}

/** Earlier in the book first; a highlight not yet placed goes last. */
export function compareCfi(a: string | null, b: string | null): number {
	if (!a || !b) return a ? -1 : b ? 1 : 0;
	const [x, y] = [startOf(a), startOf(b)];
	for (let i = 0; i < Math.max(x.length, y.length); i++) {
		const d = (x[i] ?? -1) - (y[i] ?? -1);
		if (d) return d;
	}
	return 0;
}

/** Grouped by book, in reading order within each, the book changed last first. */
export function byBook(notes: Note[]): BookNotes[] {
	const books = new Map<number, BookNotes>();
	for (const n of notes) {
		let b = books.get(n.book_id);
		if (!b) {
			b = { bookId: n.book_id, title: n.book_title, author: n.book_author, hasCover: n.book_has_cover, changedAt: n.book_changed_at, notes: [] };
			books.set(n.book_id, b);
		}
		b.notes.push(n);
	}
	const latest = (b: BookNotes) => b.notes.reduce((m, n) => (n.updated_at > m ? n.updated_at : m), '');
	return [...books.values()]
		.map((b) => ({ ...b, notes: [...b.notes].sort((p, q) => compareCfi(p.cfi, q.cfi) || p.id - q.id) }))
		.sort((p, q) => latest(q).localeCompare(latest(p)));
}

/** The notes whose passage, note or book has every word; with notes only when asked. */
export function search(notes: Note[], text: string, withNotesOnly: boolean): Note[] {
	const words = fold(text).split(/\s+/).filter(Boolean);
	return notes.filter((n) => {
		if (withNotesOnly && !n.note) return false;
		if (words.length === 0) return true;
		const hay = fold(`${n.text} ${n.note ?? ''} ${n.book_title} ${n.book_author ?? ''}`);
		return words.every((w) => hay.includes(w));
	});
}
