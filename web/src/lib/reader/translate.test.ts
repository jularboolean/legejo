import { describe, expect, it } from 'vitest';
import { canTranslate, primaryLanguage } from './translate';

describe('canTranslate', () => {
	it('goes by the primary language', () => {
		expect(primaryLanguage('fr-CA')).toBe('fr');
		expect(primaryLanguage('')).toBe(null);
		expect(canTranslate('fr', 'sv')).toBe(true);
		expect(canTranslate('sv-SE', 'sv')).toBe(false);
		expect(canTranslate('EN_gb', 'en')).toBe(false);
	});

	it('offers a translation when the book does not say its language', () => {
		expect(canTranslate(null, 'sv')).toBe(true);
	});
});
