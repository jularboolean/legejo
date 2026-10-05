import { error } from '@sveltejs/kit';
import { t } from '#lib/i18n';
import type { BookDetail } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(`/api/books/${params.id}`);
	if (!res.ok) {
		error(res.status === 404 ? 404 : 500, t('book.notFound'));
	}
	const book: BookDetail = await res.json();
	return { book };
};
