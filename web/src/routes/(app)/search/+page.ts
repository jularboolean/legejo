import type { SearchResult, TextSearchResult } from '#lib/types';
import type { PageLoad } from './$types';

const EMPTY: SearchResult = { mine: [], my_shelves: [], public: [], shelves: [], audiobooks: [] };

export const load: PageLoad = async ({ fetch, url, parent }) => {
	const q = url.searchParams.get('q')?.trim() ?? '';
	// ?in=text searches the text of the books instead, when the instance has it.
	const { fulltext } = await parent();
	const inText = fulltext && url.searchParams.get('in') === 'text';
	if (inText) {
		const text = await ask<TextSearchResult>(fetch, `/api/search/text?q=${encodeURIComponent(q)}`);
		return { q, inText, result: EMPTY, text, failed: text === undefined };
	}
	if (!q) {
		return { q, inText, result: EMPTY, text: null, failed: false };
	}
	const result = await ask<SearchResult>(fetch, `/api/search?q=${encodeURIComponent(q)}`);
	return { q, inText, result: result ?? EMPTY, text: null, failed: result === undefined };
};

/** The answer to a search; `null` when the server says no, `undefined` when
 *  no answer came at all (the request broke off or took too long), so the
 *  page can say so instead of failing whole. */
async function ask<T>(fetch: typeof globalThis.fetch, url: string): Promise<T | null | undefined> {
	try {
		const res = await fetch(url);
		return res.ok ? await res.json() : null;
	} catch {
		return undefined;
	}
}
