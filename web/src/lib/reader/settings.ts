import type {
	ReaderAlign,
	ReaderFlow,
	ReaderFont,
	ReaderSettings,
	ReaderSpread,
	ReaderTheme
} from './types';

const KEY = 'legejo.reader.settings';

export const THEMES: ReaderTheme[] = ['light', 'sepia', 'dark'];
export const FONTS: ReaderFont[] = ['book', 'serif', 'sans'];
export const FLOWS: ReaderFlow[] = ['paginated', 'scrolled'];
export const ALIGNS: ReaderAlign[] = ['book', 'left', 'justify'];
export const SPREADS: ReaderSpread[] = ['auto', 'single'];

/** Allowed ranges for the numeric settings; the settings panel reads these too. */
export const LIMITS = {
	fontSize: { min: 70, max: 220, step: 10 },
	lineHeight: { min: 1.1, max: 2.2, step: 0.1 },
	margin: { min: 2, max: 16, step: 2 }
} as const;

export function defaultSettings(): ReaderSettings {
	const media = (query: string) => typeof matchMedia === 'function' && matchMedia(query).matches;
	return {
		theme: media('(prefers-color-scheme: dark)') ? 'dark' : 'light',
		fontSize: 100,
		fontFamily: 'book',
		lineHeight: 1.5,
		// Phones have no room to spare; wide screens read better with air around the text.
		margin: media('(max-width: 40rem)') ? 6 : 8,
		flow: 'paginated',
		textAlign: 'book',
		hyphenate: false,
		spread: 'auto'
	};
}

function clamp(value: unknown, limit: { min: number; max: number }, fallback: number): number {
	const n = typeof value === 'number' && Number.isFinite(value) ? value : fallback;
	return Math.min(limit.max, Math.max(limit.min, n));
}

function oneOf<T extends string>(value: unknown, allowed: T[], fallback: T): T {
	return allowed.includes(value as T) ? (value as T) : fallback;
}

/** Fill in and sanitise settings from an untrusted source (old versions, hand edits). */
export function normalizeSettings(raw: unknown): ReaderSettings {
	const d = defaultSettings();
	const r = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
	return {
		theme: oneOf(r.theme, THEMES, d.theme),
		fontSize: Math.round(clamp(r.fontSize, LIMITS.fontSize, d.fontSize)),
		fontFamily: oneOf(r.fontFamily, FONTS, d.fontFamily),
		lineHeight: Math.round(clamp(r.lineHeight, LIMITS.lineHeight, d.lineHeight) * 10) / 10,
		margin: Math.round(clamp(r.margin, LIMITS.margin, d.margin)),
		flow: oneOf(r.flow, FLOWS, d.flow),
		textAlign: oneOf(r.textAlign, ALIGNS, d.textAlign),
		hyphenate: typeof r.hyphenate === 'boolean' ? r.hyphenate : d.hyphenate,
		spread: oneOf(r.spread, SPREADS, d.spread)
	};
}

export function loadSettings(): ReaderSettings {
	try {
		const raw = localStorage.getItem(KEY);
		return normalizeSettings(raw ? JSON.parse(raw) : null);
	} catch {
		return defaultSettings();
	}
}

export function saveSettings(settings: ReaderSettings): void {
	try {
		localStorage.setItem(KEY, JSON.stringify(settings));
	} catch {
		// Private mode or full storage: the settings just last for this visit.
	}
}
