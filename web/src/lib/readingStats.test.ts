import { describe, expect, it } from 'vitest';
import { readingStats } from './readingStats';
import type { LibraryBook } from './types';

const book = (id: number, extra: Partial<LibraryBook> = {}) =>
	({ id, title: `Book ${id}`, author: null, language: null, created_at: '2026-10-01T00:00:00Z', ...extra }) as LibraryBook;

describe('readingStats', () => {
	it('adds up the time the Kobo counted, over the books it read', () => {
		const s = readingStats([book(1, { kobo_minutes: 42 }), book(2, { kobo_minutes: 18 }), book(3), book(4, { kobo_minutes: 0 })]);
		expect(s.koboMinutes).toBe(60);
		expect(s.koboBooks).toBe(2);
	});

	it('has no Kobo time without a Kobo', () => {
		const s = readingStats([book(1), book(2)]);
		expect(s.koboMinutes).toBe(0);
		expect(s.koboBooks).toBe(0);
	});
});
