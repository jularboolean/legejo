import { error } from '@sveltejs/kit';
import { t } from '#lib/i18n';
import type { PublicBook, PublicShelf } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(
		`/api/public/${encodeURIComponent(params.owner)}/${encodeURIComponent(params.shelf)}/${params.uuid}`
	);
	if (!res.ok) {
		error(res.status === 404 ? 404 : 500, t('book.notFound'));
	}
	const detail: { shelf: PublicShelf; book: PublicBook } = await res.json();
	return { publicShelfRef: detail.shelf, publicBook: detail.book };
};
