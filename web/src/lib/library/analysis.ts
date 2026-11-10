import { en } from '#lib/i18n/en';
import { t, type MessageKey } from '#lib/i18n';

// A book's analysis as the server worked it out (GET /api/books/:id/analysis),
// its concordance and a word's forms, and what the page draws from them.

export type CurvePart = { lix: number; sentence: number; spoken: number };

export type Style = {
	words: number;
	sentences: number;
	sentence_mean: number;
	sentence_median: number;
	sentence_spread: number[];
	word_mean: number;
	long_words: number;
	variation: number | null;
	spoken: number;
	lix: number;
	level: number;
	own: { name: OwnIndex; score: number } | null;
	curve: CurvePart[];
	drift: number;
	trend: -1 | 0 | 1;
};

export type OwnIndex = 'flesch' | 'fernandez-huerta' | 'amstad' | 'gulpease';

/** The indexes by the names they go under, which are the same in every language. */
export const OWN_NAMES: Record<OwnIndex, string> = {
	flesch: 'Flesch Reading Ease',
	'fernandez-huerta': 'Fernández Huerta',
	amstad: 'Amstad',
	gulpease: 'Gulpease'
};

export type Phrase = { text: string; count: number; pmi: number };
export type Name = { name: string; count: number; presence: number[] };
export type Link = { a: number; b: number; count: number };
/** `possible`: words whose form can be something else too. */
export type Counted = { tag: string; count: number; possible?: number; examples: string[] };
/** How far the commonest words of the language reach into the book. */
export type Reach = { within: [number, number][]; need: number | null; cefr: string | null; source: string | null };
export type Group = { key: string; of: number; items: Counted[] };

export type Analysis = {
	language: string;
	style: Style;
	phrases: Phrase[];
	people: { names: Name[]; links: Link[] };
	grammar: { source: string; groups: Group[] } | null;
	reach?: Reach | null;
	/** The language's words have tables of forms. */
	forms?: boolean;
};

export type Line = { left: string; hit: string; right: string; at: number };
export type Concordance = { total: number; lines: Line[]; forms: [string, number][] };

export type FormRow = { tags: string[]; form: string; this: boolean };
export type Forms = {
	word: string;
	class: string;
	participle: string | null;
	of: string | null;
	tables: { key: string; title: string[]; rows: FormRow[] }[];
};

async function getJson<T>(url: string, fetcher: typeof fetch): Promise<T | null> {
	try {
		const res = await fetcher(url);
		return res.ok ? await res.json() : null;
	} catch {
		return null;
	}
}

export const loadAnalysis = (id: number, fetcher: typeof fetch = fetch) => getJson<Analysis>(`/api/books/${id}/analysis`, fetcher);

export const loadConcordance = (id: number, query: string, exact: boolean, fetcher: typeof fetch = fetch) =>
	getJson<Concordance>(`/api/books/${id}/analysis/concordance?q=${encodeURIComponent(query)}${exact ? '&exact=true' : ''}`, fetcher);

export const loadForms = (language: string, word: string, fetcher: typeof fetch = fetch) =>
	getJson<Forms>(`/api/analysis/forms?language=${encodeURIComponent(language)}&w=${encodeURIComponent(word)}`, fetcher);

/** The upper ends of the sentence lengths the server's spread has, as labels. */
export const SPREAD_LABELS = ['1–5', '6–10', '11–15', '16–20', '21–30', '31–40', '41+'];

/**
 * An index where higher is easier (Flesch and its kin) as a level, 1 (very
 * easy) to 5 (very hard), by Flesch's own limits.
 */
export function ownLevel(score: number): number {
	if (score >= 80) return 1;
	if (score >= 65) return 2;
	if (score >= 50) return 3;
	if (score >= 30) return 4;
	return 5;
}

/**
 * A line through the values across a box, the lowest and the highest a
 * little inside it: the path and the values' bounds.
 */
export function curvePath(values: number[], width: number, height: number): { d: string; min: number; max: number } {
	if (values.length < 2) return { d: '', min: 0, max: 0 };
	const min = Math.min(...values);
	const max = Math.max(...values);
	const span = max - min || 1;
	const pad = height * 0.1;
	const d = values
		.map((v, i) => {
			const x = (i / (values.length - 1)) * width;
			const y = height - pad - ((v - min) / span) * (height - 2 * pad);
			return `${i ? 'L' : 'M'}${x.toFixed(1)},${y.toFixed(1)}`;
		})
		.join(' ');
	return { d, min, max };
}

/** Where the i-th of n names stands on a circle, the first at the top. */
export function onCircle(i: number, n: number, cx: number, cy: number, r: number): { x: number; y: number; angle: number } {
	const angle = (i / Math.max(n, 1)) * 2 * Math.PI - Math.PI / 2;
	return { x: cx + r * Math.cos(angle), y: cy + r * Math.sin(angle), angle };
}

export type LineOrder = 'at' | 'right' | 'left';

const lastWordFirst = (text: string) => text.toLowerCase().split(/\s+/).filter(Boolean).reverse().join(' ');

/**
 * The lines in the order asked for: as in the book, by what follows the
 * word, or by what stands before it (the nearest word first).
 */
export function sortLines(lines: Line[], order: LineOrder, locale: string): Line[] {
	if (order === 'at') return [...lines].sort((a, b) => a.at - b.at);
	const strip = (s: string) => s.replace(/^[^\p{L}\p{N}]+/u, '');
	const key = order === 'right' ? (l: Line) => strip(l.right.toLowerCase()) : (l: Line) => lastWordFirst(l.left.replace(/[^\p{L}\p{N}\s]+/gu, ''));
	const collator = new Intl.Collator(locale);
	return [...lines].sort((a, b) => collator.compare(key(a), key(b)) || a.at - b.at);
}

/**
 * A grammatical tag in the reader's language. A pack may carry a tag there
 * is no text for; it is shown as it is.
 */
export function tagLabel(tag: string): string {
	const key = `analysis.tag.${tag}`;
	return key in en ? t(key as MessageKey) : tag;
}
