import { error } from '@sveltejs/kit';
import { t } from '#lib/i18n';
import type { BookDetail, Shelf } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const [bookRes, shelvesRes] = await Promise.all([
		fetch(`/api/books/${params.id}`),
		fetch('/api/shelves')
	]);
	if (!bookRes.ok) {
		error(bookRes.status === 404 ? 404 : 500, t('book.notFound'));
	}
	const book: BookDetail = await bookRes.json();
	const shelves: Shelf[] = shelvesRes.ok ? await shelvesRes.json() : [];
	return { book, shelves };
};
