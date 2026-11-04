import { describe, expect, it } from 'vitest';
import { isFinished, opensAsVisit, opensAtStart, visitCounter } from './visit';

describe('opensAsVisit', () => {
	const unread = { read_at: null, progress_percent: undefined };
	const reading = { read_at: null, progress_percent: 0.4 };
	const done = { read_at: null, progress_percent: 0.995 };
	const marked = { read_at: '2026-10-01 10:00:00', progress_percent: 0.2 };

	it('opens a book as usual from its page', () => {
		expect(opensAsVisit(unread, {})).toBe(false);
		expect(opensAsVisit(reading, {})).toBe(false);
	});

	it('is a visit from a search or a highlight', () => {
		expect(opensAsVisit(reading, { find: 'katt' })).toBe(true);
		expect(opensAsVisit(unread, { at: 7 })).toBe(true);
	});

	it('is a visit in a finished book', () => {
		expect(isFinished(done)).toBe(true);
		expect(isFinished(marked)).toBe(true);
		expect(opensAsVisit(done, {})).toBe(true);
		expect(opensAsVisit(marked, {})).toBe(true);
	});

	it('is never a visit when reading again', () => {
		expect(opensAsVisit(done, { reread: true })).toBe(false);
		expect(opensAsVisit(reading, { find: 'katt', reread: true })).toBe(false);
	});
});

describe('opensAtStart', () => {
	const reading = { read_at: null, progress_percent: 0.4 };
	const done = { read_at: null, progress_percent: 1 };

	it('opens at the position as usual', () => {
		expect(opensAtStart(reading, {})).toBe(false);
	});

	it('starts over when reading again or looking through a finished book', () => {
		expect(opensAtStart(reading, { reread: true })).toBe(true);
		expect(opensAtStart(done, {})).toBe(true);
	});

	it('goes to a highlight or a search in a finished book', () => {
		expect(opensAtStart(done, { at: 3 })).toBe(false);
		expect(opensAtStart(done, { find: 'katt' })).toBe(false);
	});
});

describe('visitCounter', () => {
	it('counts pages forward in a row', () => {
		const c = visitCounter(3);
		expect(c.turned(true)).toBe(false);
		expect(c.turned(true)).toBe(false);
		expect(c.turned(true)).toBe(true);
	});

	it('starts again after going back or jumping', () => {
		const c = visitCounter(2);
		c.turned(true);
		expect(c.turned(false)).toBe(false);
		expect(c.turned(true)).toBe(false);
		c.reset();
		expect(c.turned(true)).toBe(false);
		expect(c.turned(true)).toBe(true);
	});
});
