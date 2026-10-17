import type { SearchResult, TextSearchResult } from '#lib/types';
import type { PageLoad } from './$types';

const EMPTY: SearchResult = { mine: [], my_shelves: [], public: [], shelves: [], audiobooks: [] };

export const load: PageLoad = async ({ fetch, url, parent }) => {
	const q = url.searchParams.get('q')?.trim() ?? '';
	// ?in=text searches the text of the books instead, when the instance has it.
	const { fulltext } = await parent();
	const inText = fulltext && url.searchParams.get('in') === 'text';
	if (inText) {
		const res = await fetch(`/api/search/text?q=${encodeURIComponent(q)}`);
		const text: TextSearchResult | null = res.ok ? await res.json() : null;
		return { q, inText, result: EMPTY, text };
	}
	if (!q) {
		return { q, inText, result: EMPTY, text: null };
	}
	const res = await fetch(`/api/search?q=${encodeURIComponent(q)}`);
	const result: SearchResult = res.ok ? await res.json() : EMPTY;
	return { q, inText, result, text: null };
};
