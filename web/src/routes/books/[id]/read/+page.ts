import { error, redirect } from '@sveltejs/kit';
import { t } from '#lib/i18n';
import type { BookDetail } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(`/api/books/${params.id}`);
	if (!res.ok) {
		error(res.status === 404 ? 404 : 500, t('book.notFound'));
	}
	const book: BookDetail = await res.json();
	// EPUB and PDF books are read here; comics on a device.
	if (book.format !== 'epub' && book.format !== 'pdf') redirect(307, `/books/${params.id}`);
	return { book };
};
