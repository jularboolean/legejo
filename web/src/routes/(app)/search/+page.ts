import type { SearchResult } from '#lib/types';
import type { PageLoad } from './$types';

const EMPTY: SearchResult = { mine: [], my_shelves: [], public: [], shelves: [], audiobooks: [] };

export const load: PageLoad = async ({ fetch, url }) => {
	const q = url.searchParams.get('q')?.trim() ?? '';
	// The wider search asks a language model for related terms first; it is
	// asked for on purpose (?wide=1), never while the query is being typed.
	const wide = url.searchParams.get('wide') === '1';
	if (!q) {
		return { q, wide, result: EMPTY, wideError: null };
	}
	if (wide) {
		const res = await fetch(`/api/search/wider?q=${encodeURIComponent(q)}`);
		if (res.ok) {
			const result: SearchResult = await res.json();
			return { q, wide, result, wideError: null };
		}
		// Whatever went wrong there, the ordinary search still answers.
		const wideError: string = (await res.json().catch(() => null))?.error ?? 'model';
		const plain = await fetch(`/api/search?q=${encodeURIComponent(q)}`);
		const result: SearchResult = plain.ok ? await plain.json() : EMPTY;
		return { q, wide, result, wideError };
	}
	const res = await fetch(`/api/search?q=${encodeURIComponent(q)}`);
	const result: SearchResult = res.ok ? await res.json() : EMPTY;
	return { q, wide, result, wideError: null };
};
