import { describe, expect, it } from 'vitest';
import { sortShared } from './index';
import type { Book } from '../types';

const book = (id: number, title: string, author: string | null, first_published: number | null) =>
	({ id, title, author, first_published, published: null, created_at: `2026-01-0${id}` }) as unknown as Book;

// As the server hands them over: latest added to the shelf first.
const shelf = [
	book(3, 'Röda rummet', 'August Strindberg', 1879),
	book(1, 'Gösta Berlings saga', 'Lagerlöf, Selma', 1891),
	book(2, 'Aniara', 'Harry Martinson', null)
];
const ids = (books: Book[]) => books.map((b) => b.id);

describe('sortShared', () => {
	it('keeps the order the shelf was filled in, either way round', () => {
		expect(ids(sortShared(shelf, 'added', 'desc', 'sv'))).toEqual([3, 1, 2]);
		expect(ids(sortShared(shelf, 'added', 'asc', 'sv'))).toEqual([2, 1, 3]);
		expect(ids(shelf)).toEqual([3, 1, 2]);
	});

	it('orders by surname, by title, and by year with the undated last', () => {
		expect(ids(sortShared(shelf, 'author', 'asc', 'sv'))).toEqual([1, 2, 3]);
		expect(ids(sortShared(shelf, 'title', 'asc', 'sv'))).toEqual([2, 1, 3]);
		expect(ids(sortShared(shelf, 'year', 'desc', 'sv'))).toEqual([1, 3, 2]);
		expect(ids(sortShared(shelf, 'year', 'asc', 'sv'))).toEqual([3, 1, 2]);
	});
});
