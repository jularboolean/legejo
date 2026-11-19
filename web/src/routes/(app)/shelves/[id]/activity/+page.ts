import { error } from '@sveltejs/kit';
import { t } from '#lib/i18n';
import type { ShelfEventPage } from '#lib/shelfEvents';
import type { ShelfDetail } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const [shelfRes, eventsRes] = await Promise.all([fetch(`/api/shelves/${params.id}`), fetch(`/api/shelves/${params.id}/events`)]);
	if (!shelfRes.ok) {
		error(shelfRes.status === 404 ? 404 : 500, t('shelf.notFound'));
	}
	const shelf: ShelfDetail = await shelfRes.json();
	const page: ShelfEventPage = eventsRes.ok ? await eventsRes.json() : { entries: [], next: null };
	return { shelf, page };
};
