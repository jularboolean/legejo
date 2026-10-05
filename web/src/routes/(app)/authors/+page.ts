import type { LibraryBook } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/books');
	const books: LibraryBook[] = res.ok ? await res.json() : [];
	return { books };
};
