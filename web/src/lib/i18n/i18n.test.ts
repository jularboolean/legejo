import { describe, expect, it } from 'vitest';
import { locales, setLocale, t, type MessageKey } from '#lib/i18n';
import { en } from './en';

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe('translations', () => {
	it('have a text for every key they list, in every language', () => {
		for (const [code, messages] of Object.entries(locales)) {
			for (const [key, text] of Object.entries(messages)) {
				expect(key in en, `${code}: ${key} is not a key of the English texts`).toBe(true);
				expect(typeof text === 'string' && text.length > 0, `${code}: ${key} is empty`).toBe(true);
			}
		}
	});

	it('use only the placeholders the English text has', () => {
		for (const [code, messages] of Object.entries(locales)) {
			for (const [key, text] of Object.entries(messages)) {
				const known = placeholders(en[key as MessageKey]);
				for (const name of placeholders(text as string)) {
					expect(known, `${code}: ${key} uses {${name}}`).toContain(name);
				}
			}
		}
	});
});

describe('t', () => {
	it('fills in the parameters', () => {
		setLocale('en');
		expect(t('log.book.deleted', { title: 'Dracula' })).toBe('deleted “Dracula”');
	});

	it('falls back to English for a text the language lacks', () => {
		setLocale('sv');
		const missing = (Object.keys(en) as MessageKey[]).find((key) => !(key in locales.sv));
		if (missing) expect(t(missing)).toBe(en[missing]);
		setLocale('en');
	});

	it('gives the key back, with or without parameters, when there is no text at all', () => {
		const unknown = 'log.no.such.action' as MessageKey;
		expect(t(unknown)).toBe(unknown);
		expect(t(unknown, { title: 'x' })).toBe(unknown);
	});
});
