import { describe, expect, it } from 'vitest';
import { findInPage, readItems, type Piece } from './find';

const line = (str: string): Piece => ({ str, eol: true });

describe('finding words in a PDF page', () => {
	it('finds a word broken into pieces, and says where the pieces are', () => {
		const [hit, ...rest] = findInPage([{ str: 'ett kniv' }, { str: 'blad av ljus' }], 'knivblad');
		expect(rest).toEqual([]);
		expect(hit.from).toEqual({ piece: 0, offset: 4 });
		expect(hit.to).toEqual({ piece: 1, offset: 4 });
		expect(hit.match).toBe('knivblad');
		expect([hit.before, hit.after]).toEqual(['ett ', ' av ljus']);
	});

	it('takes no notice of case and accents', () => {
		expect(findInPage([{ str: 'Fänrik Ståls sägner' }], 'STALS')[0].match).toBe('Ståls');
		expect(findInPage([{ str: 'Fänrik' }], 'fänrik')[0].match).toBe('Fa\u0308nrik');
	});

	it('finds words over a line end', () => {
		expect(findInPage([line('röda'), line('stugan')], 'röda stugan')[0].match).toBe('röda stugan');
		expect(findInPage([line('hänvis-'), line('ningar')], 'hänvisningar')[0].match).toBe('hänvisningar');
		expect(findInPage([line('me­'), line('delålders')], 'medelålders')[0].match).toBe('medelålders');
	});

	it('leaves a hyphen that belongs to the words', () => {
		expect(findInPage([line('Nord-'), line('Sverige')], 'nord-sverige')[0].match).toBe('Nord-Sverige');
		expect(findInPage([{ str: '1914-1918' }], '1914-1918')).toHaveLength(1);
	});

	it('finds every match, and nothing in an empty search', () => {
		expect(findInPage([{ str: 'ord och ord och ord' }], 'ord')).toHaveLength(3);
		expect(findInPage([{ str: 'ord' }], '  ')).toEqual([]);
		expect(findInPage([], 'ord')).toEqual([]);
	});
});

describe('reading the text of a PDF page', () => {
	it('puts the parts of the stream together, without looping over it', async () => {
		const parts = [{ items: ['ett', 'två'] }, { items: [] }, { items: ['tre'] }];
		const stream = {
			getReader: () => ({ read: async () => (parts.length ? { done: false, value: parts.shift() } : { done: true }) })
		} as unknown as ReadableStream<{ items: string[] }>;
		expect(await readItems(stream)).toEqual(['ett', 'två', 'tre']);
	});
});
