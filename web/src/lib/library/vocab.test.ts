import { describe, expect, it } from 'vitest';
import {
	ankiDeck,
	appendUnique,
	asAnki,
	asText,
	knownCover,
	loadKnownWords,
	loadSearch,
	loadVocab,
	percent,
	readingLevel,
	shownLevel,
	unknownPrefix,
	type StudyRow,
	type Vocab
} from './vocab';

/** A fetch that answers every request with the same JSON body, counting the calls. */
function fakeFetch(body: unknown, ok = true) {
	const calls: string[] = [];
	const fetcher = (async (input: RequestInfo | URL) => {
		calls.push(String(input));
		return { ok, json: async () => body } as Response;
	}) as typeof fetch;
	return { fetcher, calls };
}

const row = (stem: string, extra: Partial<StudyRow> = {}): StudyRow => ({
	stem,
	word: stem,
	forms: [stem],
	count: 1,
	name: false,
	known: false,
	english: null,
	found_as: null,
	class: null,
	...extra
});

describe('readingLevel', () => {
	it('follows the coverage thresholds of 80, 90, 95 and 98 %', () => {
		expect(readingLevel(0.52)).toBe('hard');
		expect(readingLevel(0.8)).toBe('tough');
		expect(readingLevel(0.93)).toBe('support');
		expect(readingLevel(0.95)).toBe('good');
		expect(readingLevel(0.985)).toBe('easy');
	});
});

describe('shownLevel', () => {
	it('agrees with the percentage shown, which is rounded', () => {
		for (const share of [0.949, 0.979, 0.996, 0.895, 0.795]) {
			expect(shownLevel(share)).toBe(readingLevel(percent(share) / 100));
		}
		// 94.9 % shows as 95 %, and 95 % reads well.
		expect(percent(0.949)).toBe(95);
		expect(shownLevel(0.949)).toBe('good');
		expect(readingLevel(0.949)).toBe('support');
	});
});

describe('appendUnique', () => {
	it('adds the rows a list does not have yet, in their order', () => {
		const rows = [{ id: 1 }, { id: 2 }];
		expect(appendUnique(rows, [{ id: 2 }, { id: 3 }, { id: 1 }, { id: 4 }], (r) => r.id)).toEqual([{ id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }]);
	});

	it('leaves the list as it is when nothing is new', () => {
		const rows = [{ stem: 'a' }];
		expect(appendUnique(rows, [{ stem: 'a' }], (r) => r.stem)).toEqual(rows);
		expect(appendUnique([], [{ stem: 'a' }], (r) => r.stem)).toEqual([{ stem: 'a' }]);
	});
});

describe('unknownPrefix', () => {
	it('counts the unknown words from the top down to each row', () => {
		const words = ['a', 'b', 'c', 'd'].map((stem) => ({ stem }));
		const upTo = unknownPrefix(words, new Set(['a', 'c']));
		expect([...upTo]).toEqual([
			['a', 0],
			['b', 1],
			['c', 1],
			['d', 2]
		]);
	});

	it('is empty for an empty list', () => {
		expect(unknownPrefix([], new Set()).size).toBe(0);
	});
});

describe('loadVocab', () => {
	it('shares one request between concurrent calls for the same book', async () => {
		const { fetcher, calls } = fakeFetch({ language: 'eo', words: 1 });
		const [a, b] = await Promise.all([loadVocab(1, fetcher), loadVocab(1, fetcher)]);
		expect(calls).toEqual(['/api/books/1/vocabulary']);
		expect(a).toEqual({ language: 'eo', words: 1 });
		expect(b).toBe(a);
	});

	it('asks again once the request has settled', async () => {
		const { fetcher, calls } = fakeFetch({ language: 'eo' });
		await loadVocab(2, fetcher);
		await loadVocab(2, fetcher);
		expect(calls).toHaveLength(2);
	});

	it('keeps different books apart', async () => {
		const { fetcher, calls } = fakeFetch({ language: 'eo' });
		await Promise.all([loadVocab(3, fetcher), loadVocab(4, fetcher)]);
		expect(calls).toEqual(['/api/books/3/vocabulary', '/api/books/4/vocabulary']);
	});

	it('gives null for an answer that is not ok, and forgets the request', async () => {
		const { fetcher, calls } = fakeFetch({}, false);
		expect(await loadVocab(5, fetcher)).toBeNull();
		expect(await loadVocab(5, fetcher)).toBeNull();
		expect(calls).toHaveLength(2);
	});
});

describe('knownCover', () => {
	const vocab = (study: StudyRow[] | null, words: number): Vocab => ({
		language: 'eo',
		words,
		stems: 0,
		hapax: 0,
		coverage: {},
		curve: [],
		books_in_language: 0,
		rated_from: 0,
		difficulty: null,
		short: false,
		characteristic: [],
		common: [],
		distinctive_from: 0,
		names: [],
		study,
		known_share: null,
		study_source: null
	});

	it('is 0 without a study list', () => {
		expect(knownCover(vocab(null, 100), new Set(['a']))).toBe(0);
	});

	it('leaves names out of both sides', () => {
		const v = vocab([row('a', { count: 60 }), row('b', { count: 20 }), row('zamenhof', { count: 20, name: true })], 100);
		expect(knownCover(v, new Set(['a']))).toBeCloseTo(0.75);
		expect(knownCover(v, new Set(['a', 'zamenhof']))).toBeCloseTo(0.75);
	});

	it('is 1 when every word to learn is known', () => {
		const v = vocab([row('a', { count: 70 }), row('b', { count: 30 })], 100);
		expect(knownCover(v, new Set(['a', 'b']))).toBe(1);
	});

	it('is 0, not NaN, for a book of nothing but names', () => {
		const v = vocab([row('zamenhof', { count: 10, name: true })], 10);
		expect(knownCover(v, new Set())).toBe(0);
	});
});

describe('asText', () => {
	it('lines the meanings up after the heading, with the forms in brackets', () => {
		const text = asText(['Title', 'Source'], [
			{ word: 'hundo', forms: ['hundoj'], english: 'dog' },
			{ word: 'kato', english: null }
		]);
		expect(text).toBe(['Title', 'Source', '', 'hundo (hundoj)   dog', 'kato', ''].join('\n'));
	});
});

describe('asAnki', () => {
	it('writes the headers Anki reads and tab-separated rows', () => {
		const text = asAnki('Legejo::Esperanto', 'legejo eo', [{ word: 'hundo', forms: ['hundoj'], english: 'dog' }]);
		const lines = text.split('\n');
		expect(lines.slice(0, 5)).toEqual(['#separator:tab', '#html:true', '#notetype:Basic', '#deck:Legejo::Esperanto', '#tags column:3']);
		expect(lines[5].split('\t')).toEqual(['hundo', 'dog<br><small>hundoj</small>', 'legejo eo']);
		expect(lines.at(-1)).toBe('');
	});

	it('escapes tabs, line breaks and HTML in the fields, and adds the about row', () => {
		const text = asAnki('Legejo::X', 'legejo', [{ word: 'a\tb', english: '<c> & d\ne' }], ['About', 'From <x>']);
		const [first, about] = text.split('\n').slice(5);
		expect(first.split('\t')).toEqual(['a b', '&lt;c> &amp; d e', 'legejo']);
		expect(about.split('\t')).toEqual(['About', 'From &lt;x>', 'legejo']);
	});

	it('nests the deck under Legejo and keeps a name from nesting further', () => {
		expect(ankiDeck('Esperanto')).toBe('Legejo::Esperanto');
		expect(ankiDeck('A::B')).toBe('Legejo::A:B');
	});
});

describe('loadKnownWords', () => {
	it('asks for the language alone when nothing else is set', async () => {
		const { fetcher, calls } = fakeFetch({ total: 0, words: [] });
		await loadKnownWords('eo', {}, fetcher);
		expect(calls).toEqual(['/api/vocabulary/known/words?language=eo']);
	});

	it('leaves out "all" classes and an offset of 0, and keeps the rest', async () => {
		const { fetcher, calls } = fakeFetch({ total: 0, words: [] });
		await loadKnownWords('eo', { q: 'hund', sort: 'alpha', class: 'all', offset: 0, limit: 20 }, fetcher);
		expect(calls[0]).toBe('/api/vocabulary/known/words?language=eo&q=hund&sort=alpha&limit=20');
		await loadKnownWords('eo', { class: 'verb', offset: 20 }, fetcher);
		expect(calls[1]).toBe('/api/vocabulary/known/words?language=eo&class=verb&offset=20');
	});

	it('gives null when the server does not answer well', async () => {
		const { fetcher } = fakeFetch({}, false);
		expect(await loadKnownWords('eo', {}, fetcher)).toBeNull();
	});
});

describe('loadSearch', () => {
	it('asks the search endpoint and gives the matches back', async () => {
		const match = { stem: 'hund', word: 'hundo', english: 'dog', class: 'noun', known: false, books: 1, book_ids: [1], count: 3 };
		const { fetcher, calls } = fakeFetch({ matches: [match] });
		expect(await loadSearch('eo', 'hun do', fetcher)).toEqual([match]);
		expect(calls).toEqual(['/api/vocabulary/search?language=eo&q=hun+do']);
	});

	it('gives null when the server does not answer well', async () => {
		const { fetcher } = fakeFetch({}, false);
		expect(await loadSearch('eo', 'x', fetcher)).toBeNull();
	});
});
