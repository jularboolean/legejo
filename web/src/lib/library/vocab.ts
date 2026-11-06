// A book's vocabulary as the server counted it (GET /api/books/:id/vocabulary),
// and the reader's known words (PUT /api/vocabulary/known).

export type StudyRow = {
	stem: string;
	word: string;
	forms: string[];
	count: number;
	name: boolean;
	known: boolean;
	english: string | null;
	found_as: string | null;
};

export type Vocab = {
	language: string;
	words: number;
	stems: number;
	hapax: number;
	coverage: Record<string, number>;
	curve: [number, number][];
	books_in_language: number;
	rated_from: number;
	difficulty: { level: number; compared_with: number } | null;
	short: boolean;
	characteristic: string[];
	names: string[];
	study: StudyRow[] | null;
	known_share: number | null;
	study_source: string | null;
};

export async function loadVocab(id: number, fetcher: typeof fetch = fetch): Promise<Vocab | null> {
	try {
		const res = await fetcher(`/api/books/${id}/vocabulary`);
		return res.ok ? await res.json() : null;
	} catch {
		return null;
	}
}

/** Mark a word known or not; false when the server did not take it. */
export async function setKnown(language: string, stem: string, known: boolean): Promise<boolean> {
	try {
		const res = await fetch('/api/vocabulary/known', {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ language, stem, known })
		});
		return res.ok;
	} catch {
		return false;
	}
}

/** Share of the book's words there are to learn (names left out) that the known words cover. */
export function knownCover(v: Vocab, known: Set<string>): number {
	if (!v.study) return 0;
	let covered = 0;
	let names = 0;
	for (const w of v.study) {
		if (w.name) names += w.count;
		else if (known.has(w.stem)) covered += w.count;
	}
	return covered / Math.max(1, v.words - names);
}

/** The language's name as it stands inside a sentence ("svenska", "Swedish"). */
export function languageInSentence(code: string, locale: string): string {
	try {
		return new Intl.DisplayNames([locale], { type: 'language' }).of(code) ?? code;
	} catch {
		return code;
	}
}

export const fmt = (n: number, locale: string) => n.toLocaleString(locale);
export const percent = (x: number) => Math.round(x * 100);
/** For CSS: no space before the sign. */
export const css = (x: number) => `${(x * 100).toFixed(1)}%`;
