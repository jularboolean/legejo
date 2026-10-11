import { describe, expect, it } from 'vitest';
import { bookKey, bookUrls } from './keys';

describe('bookKey', () => {
	it('knows the requests the reader makes for a book', () => {
		expect(bookKey('/api/books/12')).toEqual({ id: '12', kind: 'detail', url: '/api/books/12' });
		expect(bookKey('/api/books/12/file')?.kind).toBe('file');
		expect(bookKey('/api/books/12/cover')?.kind).toBe('cover');
		expect(bookKey('/api/books/12/progress')?.kind).toBe('progress');
		expect(bookKey('/api/books/12/annotations')?.kind).toBe('annotations');
	});

	it('leaves everything else to the network', () => {
		for (const path of ['/api/books', '/api/books/12/annotations/export', '/api/books/12/kindle', '/api/books/x/file', '/api/shelves/12', '/api/books/12/cover/openlibrary']) {
			expect(bookKey(path)).toBeNull();
		}
	});

	it('agrees with the addresses a saved book is kept under', () => {
		for (const url of Object.values(bookUrls(7))) expect(bookKey(url)?.url).toBe(url);
		expect(Object.keys(bookUrls(7)).at(-1)).toBe('file');
	});
});
