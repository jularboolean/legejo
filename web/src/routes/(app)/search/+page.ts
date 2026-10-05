import type { SearchResult } from '#lib/types';
import type { PageLoad } from './$types';

const EMPTY: SearchResult = { mine: [], public: [], shelves: [] };

export const load: PageLoad = async ({ fetch, url }) => {
	const q = url.searchParams.get('q')?.trim() ?? '';
	if (!q) {
		return { q, result: EMPTY };
	}
	const res = await fetch(`/api/search?q=${encodeURIComponent(q)}`);
	const result: SearchResult = res.ok ? await res.json() : EMPTY;
	return { q, result };
};
