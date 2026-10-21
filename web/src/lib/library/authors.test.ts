import { describe, expect, it } from 'vitest';
import { collectAuthors, initial } from './authors';
import type { LibraryBook } from '../types';

const books = (authors: string[]) => authors.map((author, id) => ({ id, author }) as unknown as LibraryBook);

describe('the letters of the authors page', () => {
	it('file a letter the language sorts as another under that one, so each letter comes once', () => {
		const library = books(['Slavoj Žižek', 'L. L. Zamenhof', 'Stefan Zweig', 'Émile Zola', 'Albert Engström']);
		const letters = collectAuthors(library, 'sv').map((a) => initial(a.sort, 'sv'));
		expect(letters).toEqual(['E', 'Z', 'Z', 'Z', 'Z']);
	});

	it('keep the letters a language has of its own', () => {
		expect(initial('Öberg Anna', 'sv')).toBe('Ö');
		expect(initial('Åsbrink Elisabeth', 'sv')).toBe('Å');
		expect(initial('Ĉe Iu', 'eo')).toBe('Ĉ');
		expect(initial('Émile Zola', 'sv')).toBe('E');
		expect(initial('1984', 'sv')).toBe('#');
	});
});
