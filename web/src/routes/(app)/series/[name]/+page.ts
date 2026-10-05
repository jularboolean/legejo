import type { Book } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(`/api/series/${encodeURIComponent(params.name)}`);
	const books: Book[] = res.ok ? await res.json() : [];
	return { seriesName: params.name, seriesBooks: books };
};
