import { describe, expect, it } from 'vitest';
import { groupBySeries } from './index';
import type { LibraryBook } from '../types';

function book(id: number, title: string, series: string | null, index: number | null, cover = true): LibraryBook {
	return { id, title, series, series_index: index, has_cover: cover } as LibraryBook;
}

describe('groupBySeries', () => {
	it('gives one group per series, in the order of each series’ first book', () => {
		const { series, loose } = groupBySeries(
			[book(1, 'C', 'Moomins', 3), book(2, 'Alone', null, null), book(3, 'X', 'Dune', 1), book(4, 'A', 'Moomins', 1)],
			'en'
		);
		expect(series.map((s) => s.name)).toEqual(['Moomins', 'Dune']);
		expect(series[0].books.map((b) => b.id)).toEqual([4, 1]);
		expect(loose.map((b) => b.id)).toEqual([2]);
	});

	it('puts numberless books last and shows the first book that has a cover', () => {
		const { series } = groupBySeries(
			[book(1, 'B', 'S', null), book(2, 'A', 'S', 1, false), book(3, 'C', 'S', 2)],
			'en'
		);
		expect(series[0].books.map((b) => b.id)).toEqual([2, 3, 1]);
		expect(series[0].cover.id).toBe(3);
	});

	it('takes a blank series as none, and a name with spaces around as the same series', () => {
		const { series, loose } = groupBySeries([book(1, 'A', '  ', null), book(2, 'B', 'S ', 1), book(3, 'C', 'S', 2)], 'en');
		expect(loose.map((b) => b.id)).toEqual([1]);
		expect(series).toHaveLength(1);
		expect(series[0].books).toHaveLength(2);
	});

	it('takes names that differ only in case, also beyond ASCII, as one series', () => {
		const { series } = groupBySeries([book(1, 'A', 'Det Stora Århundradet', 1), book(2, 'B', 'Det stora århundradet', 2)], 'sv');
		expect(series.map((s) => [s.name, s.books.length])).toEqual([['Det Stora Århundradet', 2]]);
	});
});
