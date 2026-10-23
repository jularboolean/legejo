import { describe, expect, it } from 'vitest';
import { minutesFor, roughly } from './readingTime';

describe('reading time', () => {
	it('is the length at the pace', () => {
		expect(minutesFor(418_700, 1135)).toBeCloseTo(368.9, 1);
		expect(minutesFor(1000, 0)).toBe(0);
		expect(minutesFor(0, 1100)).toBe(0);
	});

	it('is said as roughly as it is known', () => {
		expect(roughly(3.4)).toBe('3 min');
		expect(roughly(0.2)).toBe('1 min');
		expect(roughly(23)).toBe('25 min');
		expect(roughly(368.9)).toBe('6 h 10 min');
		expect(roughly(731)).toBe('12 h 15 min');
		expect(roughly(241)).toBe('4 h');
		expect(roughly(0)).toBeNull();
		expect(roughly(NaN)).toBeNull();
	});
});
