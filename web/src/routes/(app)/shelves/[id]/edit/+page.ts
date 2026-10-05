import { error } from '@sveltejs/kit';
import { t } from '#lib/i18n';
import type { ShelfDetail } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(`/api/shelves/${params.id}`);
	if (!res.ok) {
		error(res.status === 404 ? 404 : 500, t('shelf.notFound'));
	}
	const shelf: ShelfDetail = await res.json();
	return { shelf };
};
