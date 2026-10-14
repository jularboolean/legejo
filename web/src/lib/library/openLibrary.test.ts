import { describe, expect, it } from 'vitest';
import { onOpenLibrary } from './index';

describe('onOpenLibrary', () => {
	it('links an English book, however its language is written, and one without a language', () => {
		for (const language of ['en', 'eng', 'en-US', 'EN-US', null, '']) {
			expect(onOpenLibrary({ isbn: '9780451495136', language })).toBe(true);
		}
	});

	it('leaves other languages, and a book without an ISBN, without the link', () => {
		for (const language of ['sv', 'swe', 'sv-SE', 'fi', 'de']) {
			expect(onOpenLibrary({ isbn: '9789100129392', language })).toBe(false);
		}
		expect(onOpenLibrary({ isbn: null, language: 'en' })).toBe(false);
		expect(onOpenLibrary({ isbn: '  ', language: 'en' })).toBe(false);
	});
});
