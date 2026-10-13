import type { Book } from '#lib/types';
import { listFrom } from '#lib/loaded';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(`/api/series/${encodeURIComponent(params.name)}`);
	const books = await listFrom<Book>(res);
	return { seriesName: params.name, seriesBooks: books };
};
