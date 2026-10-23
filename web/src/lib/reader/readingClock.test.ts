import { describe, expect, it } from 'vitest';
import { IDLE_MS, countable } from './readingClock';

describe('countable', () => {
	it('counts a tick while the reader is active and the page in view', () => {
		expect(countable(5000, 1000, true, false)).toBe(5);
	});

	it('stops once the reader has been idle too long', () => {
		expect(countable(5000, IDLE_MS + 1, true, false)).toBe(0);
		expect(countable(5000, IDLE_MS - 1, true, false)).toBe(5);
	});

	it('goes on while the book is read aloud', () => {
		expect(countable(5000, IDLE_MS * 4, true, true)).toBe(5);
	});

	it('counts nothing out of view, and one tick at most after the device slept', () => {
		expect(countable(5000, 0, false, false)).toBe(0);
		expect(countable(3_600_000, 0, true, false)).toBe(5);
		expect(countable(-10, 0, true, false)).toBe(0);
	});
});
