import { describe, expect, it } from 'vitest';
import { readingLevel } from './vocab';

describe('readingLevel', () => {
	it('follows the coverage thresholds of 80, 90, 95 and 98 %', () => {
		expect(readingLevel(0.52)).toBe('hard');
		expect(readingLevel(0.8)).toBe('tough');
		expect(readingLevel(0.93)).toBe('support');
		expect(readingLevel(0.95)).toBe('good');
		expect(readingLevel(0.985)).toBe('easy');
	});
});
