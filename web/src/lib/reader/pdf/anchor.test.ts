import { describe, expect, it } from 'vitest';
import { compareCfi } from '#lib/notes';
import { decodePassage, encodePage, encodePassage, mergeLines, pageOf } from './anchor';

describe('PDF places', () => {
	it('gives the page of a position and of a passage', () => {
		expect(encodePage(12)).toBe('pdf:12');
		expect(pageOf('pdf:12')).toBe(12);
		expect(pageOf('pdf:3:0.1,0.1,0.02,0.5')).toBe(3);
		expect(pageOf('epubcfi(/6/4!/4/2/1:0)')).toBeNull();
		expect(pageOf(null)).toBeNull();
	});

	it('keeps a passage over two pages through a round trip, to four decimals', () => {
		const passage = [
			{ page: 12, rects: [{ top: 0.123456, left: 0.08, height: 0.0221, width: 0.7351 }] },
			{ page: 13, rects: [{ top: 0.05, left: 0.08, height: 0.0221, width: 0.5 }] }
		];
		const cfi = encodePassage(passage);
		expect(cfi).toBe('pdf:12:0.1235,0.08,0.0221,0.7351|13:0.05,0.08,0.0221,0.5');
		expect(decodePassage(cfi)).toEqual([
			{ page: 12, rects: [{ top: 0.1235, left: 0.08, height: 0.0221, width: 0.7351 }] },
			{ page: 13, rects: [{ top: 0.05, left: 0.08, height: 0.0221, width: 0.5 }] }
		]);
	});

	it('takes nothing that is not a passage', () => {
		expect(decodePassage('pdf:12')).toBeNull();
		expect(decodePassage('pdf:x:1,2,3,4')).toBeNull();
		expect(decodePassage('pdf:2:1,2,3')).toBeNull();
		expect(decodePassage('epubcfi(/6/4!/4/2,/1:0,/1:9)')).toBeNull();
	});

	it('sorts highlights by page, then down the page', () => {
		const a = encodePassage([{ page: 2, rects: [{ top: 0.5, left: 0.1, height: 0.02, width: 0.3 }] }]);
		const b = encodePassage([{ page: 2, rects: [{ top: 0.1, left: 0.1, height: 0.02, width: 0.3 }] }]);
		const c = encodePassage([{ page: 10, rects: [{ top: 0.05, left: 0.1, height: 0.02, width: 0.3 }] }]);
		expect([c, a, b].sort(compareCfi)).toEqual([b, a, c]);
	});
});

describe('mergeLines', () => {
	it('makes the runs of one line one rectangle, in reading order', () => {
		const lines = mergeLines([
			{ top: 0.2, left: 0.5, height: 0.02, width: 0.2 },
			{ top: 0.1, left: 0.4, height: 0.02, width: 0.3 },
			{ top: 0.101, left: 0.1, height: 0.019, width: 0.3 },
			{ top: 0.2, left: 0.1, height: 0.02, width: 0.0 }
		]);
		const expected = [
			{ top: 0.1, left: 0.1, height: 0.02, width: 0.6 },
			{ top: 0.2, left: 0.5, height: 0.02, width: 0.2 }
		];
		expect(lines).toHaveLength(expected.length);
		lines.forEach((line, i) => {
			for (const key of ['top', 'left', 'height', 'width'] as const) expect(line[key]).toBeCloseTo(expected[i][key], 6);
		});
	});
});
