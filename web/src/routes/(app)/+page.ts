import type { LibraryBook } from '#lib/types';
import { listFrom } from '#lib/loaded';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/books');
	const books = await listFrom<LibraryBook>(res);
	return { books };
};
