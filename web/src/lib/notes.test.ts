import { describe, expect, it } from 'vitest';
import { byBook, compareCfi, search, type Note } from './notes';

const note = (id: number, book: number, cfi: string | null, text: string, extra: Partial<Note> = {}): Note => ({
	id,
	book_id: book,
	source: 'web',
	cfi,
	text,
	note: null,
	color: null,
	created_at: '2026-10-01T00:00:00.000Z',
	updated_at: '2026-10-01T00:00:00.000Z',
	book_title: `Book ${book}`,
	book_author: null,
	book_has_cover: false,
	book_changed_at: '2026-10-01T00:00:00.000Z',
	...extra
});

describe('compareCfi', () => {
	it('puts what comes first in the book first', () => {
		expect(compareCfi('epubcfi(/6/4[c1]!/4/2,/1:0,/1:9)', 'epubcfi(/6/4[c1]!/4/10,/1:0,/1:3)')).toBeLessThan(0);
		expect(compareCfi('epubcfi(/6/12!/4/2,/1:0,/1:9)', 'epubcfi(/6/4!/4/2,/1:0,/1:9)')).toBeGreaterThan(0);
		// Same paragraph: the offset decides; 10 after 9, not before it.
		expect(compareCfi('epubcfi(/6/4!/4/2,/1:10,/1:20)', 'epubcfi(/6/4!/4/2,/1:9,/1:20)')).toBeGreaterThan(0);
		expect(compareCfi(null, 'epubcfi(/6/4!/4/2,/1:0,/1:9)')).toBeGreaterThan(0);
	});
});

describe('byBook', () => {
	it('groups by book in reading order, the book changed last first', () => {
		const groups = byBook([
			note(1, 7, 'epubcfi(/6/8!/4/2,/1:0,/1:4)', 'later'),
			note(2, 7, 'epubcfi(/6/4!/4/2,/1:0,/1:4)', 'earlier'),
			note(3, 9, 'epubcfi(/6/4!/4/2,/1:0,/1:4)', 'other', { updated_at: '2026-10-09T00:00:00.000Z' })
		]);
		expect(groups.map((g) => g.bookId)).toEqual([9, 7]);
		expect(groups[1].notes.map((n) => n.text)).toEqual(['earlier', 'later']);
	});
});

describe('search', () => {
	const notes = [note(1, 1, null, 'Det var en afton', { note: 'Början' }), note(2, 2, null, 'Stockholm', { book_title: 'Röda rummet' })];
	it('finds words in passage, note and book, whatever the accents', () => {
		expect(search(notes, 'borjan', false).map((n) => n.id)).toEqual([1]);
		expect(search(notes, 'roda', false).map((n) => n.id)).toEqual([2]);
		expect(search(notes, '', true).map((n) => n.id)).toEqual([1]);
	});
});
