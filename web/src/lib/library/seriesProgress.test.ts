import { describe, expect, it } from 'vitest';
import { seriesProgress } from './index';
import type { Book } from '../types';

function book(id: number, index: number | null, state: 'unread' | 'reading' | 'finished' = 'unread'): Book {
	return {
		id,
		series_index: index,
		read_at: state === 'finished' ? '2026-01-01T00:00:00Z' : null,
		progress_percent: state === 'reading' ? 0.4 : undefined
	} as Book;
}

describe('seriesProgress', () => {
	it('goes on with the book being read before the first unread one', () => {
		const p = seriesProgress([book(1, 1, 'finished'), book(2, 2), book(3, 3, 'reading')]);
		expect(p.finished).toBe(1);
		expect(p.total).toBe(3);
		expect(p.next?.id).toBe(3);
	});

	it('takes the first unread when nothing is being read, and nothing when all are read', () => {
		expect(seriesProgress([book(1, 1, 'finished'), book(2, 2), book(3, 3)]).next?.id).toBe(2);
		expect(seriesProgress([book(1, 1, 'finished')]).next).toBeNull();
	});

	it('names the missing whole numbers below the highest', () => {
		expect(seriesProgress([book(1, 1), book(2, 2), book(4, 5), book(5, 2.5), book(6, null)]).gaps).toEqual([3, 4]);
		expect(seriesProgress([book(1, 1990), book(2, 1994)]).gaps).toEqual([]);
	});
});
