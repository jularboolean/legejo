import { en, type MessageKey } from './en';
import { sv } from './sv';
import { fi } from './fi';
import { eo } from './eo';
import { fr } from './fr';
import { de } from './de';
import { es } from './es';

// Translations are keyed by locale (Partial<typeof en> — missing keys fall
// back to English). To add a language: create the file, register it here and
// in LANGUAGES below.
export const locales: Record<string, Partial<Record<MessageKey, string>>> = { en, sv, fi, eo, fr, de, es };

/// Shown in the account page's language picker, in each language's own name.
export const LANGUAGES: { code: string; label: string }[] = [
	{ code: 'en', label: 'English' },
	{ code: 'sv', label: 'Svenska' },
	{ code: 'fi', label: 'Suomi' },
	{ code: 'eo', label: 'Esperanto' },
	{ code: 'fr', label: 'Français' },
	{ code: 'de', label: 'Deutsch' },
	{ code: 'es', label: 'Español' }
];

// A rune, so every t() call in a template re-renders when the locale changes.
let current = $state('en');

export function setLocale(locale: string) {
	if (locale in locales) current = locale;
}

export function getLocale(): string {
	return current;
}

export function t(key: MessageKey, params?: Record<string, string | number>): string {
	// A key without a text (an action logged by a newer server, say) is
	// shown as it is rather than breaking the page.
	let msg: string = locales[current][key] ?? en[key] ?? key;
	if (params) {
		for (const [name, value] of Object.entries(params)) {
			msg = msg.replaceAll(`{${name}}`, String(value));
		}
	}
	return msg;
}
