import { describe, expect, it } from 'vitest';
import { altText, cardQuote, fileName, fit, postText, within, wrap } from './quotecard';

// Every character 10 units wide, at any size; or `size / 2` wide at a size.
const mono = (s: string) => s.length * 10;
const atSize = (size: number) => (s: string) => (s.length * size) / 2;

describe('cardQuote', () => {
	it('evens out whitespace', () => {
		expect(cardQuote('  It was\n  a dark\tnight. ')).toBe('It was a dark night.');
	});

	it('cuts a long passage at a word, with an ellipsis', () => {
		const out = cardQuote('one two three four five six seven', 20);
		expect(out).toBe('one two three four…');
		expect(out.length).toBeLessThanOrEqual(21);
	});

	it('leaves no comma or stop before the ellipsis', () => {
		expect(cardQuote('one two, three four', 9)).toBe('one two…');
	});

	it('cuts inside a word when there is no space to cut at', () => {
		expect(cardQuote('abcdefghijklmnop', 5)).toBe('abcde…');
	});
});

describe('wrap', () => {
	it('breaks at words to fit the width', () => {
		expect(wrap('aa bb cc dd', 50, mono)).toEqual(['aa bb', 'cc dd']);
	});

	it('lets a word wider than a line stand alone', () => {
		expect(wrap('a verylongword b', 50, mono)).toEqual(['a', 'verylongword', 'b']);
	});

	it('gives nothing for nothing', () => {
		expect(wrap('', 50, mono)).toEqual([]);
	});
});

describe('fit', () => {
	it('takes the largest size that fits', () => {
		expect(fit('short', 1000, 1000, atSize).size).toBe(54);
	});

	it('goes down in size for a long passage', () => {
		const long = 'word '.repeat(120).trim();
		const { size, lines } = fit(long, 700, 900, atSize);
		expect(size).toBeLessThan(54);
		expect(lines.length * size * 1.45).toBeLessThanOrEqual(900);
	});

	it('settles for the smallest size when nothing fits', () => {
		expect(fit('word '.repeat(2000), 300, 100, atSize).size).toBe(26);
	});
});

describe('the words beside the picture', () => {
	it('put the sharer’s own first', () => {
		expect(postText(' So true. ', 'From Dracula by Bram Stoker')).toBe('So true.\n\nFrom Dracula by Bram Stoker');
		expect(postText('', 'From Dracula')).toBe('From Dracula');
	});

	it('describe the picture with the passage and the book', () => {
		expect(altText('Listen to them.', 'Dracula', 'Bram Stoker')).toBe('“Listen to them.” — Dracula, Bram Stoker');
		expect(altText('Listen.', 'Dracula', null)).toBe('“Listen.” — Dracula');
	});

	it('name the file after the title', () => {
		expect(fileName('Röda rummet')).toBe('roda-rummet-quote.png');
		expect(fileName('…')).toBe('quote-quote.png');
	});
});

describe('within', () => {
	it('gives what the work comes to', async () => {
		await expect(within(Promise.resolve('ok'), 1000, 'late')).resolves.toBe('ok');
	});

	it('does not wait for work that never ends', async () => {
		await expect(within(new Promise<string>(() => {}), 5, 'late')).resolves.toBe('late');
	});

	it('passes a failure on', async () => {
		await expect(within(Promise.reject(new Error('no')), 1000, 'late')).rejects.toThrow('no');
	});
});
