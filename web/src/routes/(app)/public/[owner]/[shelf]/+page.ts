import { error } from '@sveltejs/kit';
import { t } from '#lib/i18n';
import type { PublicShelfDetail } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(
		`/api/public/${encodeURIComponent(params.owner)}/${encodeURIComponent(params.shelf)}`
	);
	if (!res.ok) {
		error(res.status === 404 ? 404 : 500, t('public.notFound'));
	}
	const shelf: PublicShelfDetail = await res.json();
	return { publicShelf: shelf };
};
