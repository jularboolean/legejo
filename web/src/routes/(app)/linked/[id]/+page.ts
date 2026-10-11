import { error } from '@sveltejs/kit';
import { t } from '#lib/i18n';
import type { LinkedBook, LinkedShelf } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(`/api/linked-shelves/${params.id}`);
	if (res.status === 404) error(404, t('shelf.notFound'));
	if (!res.ok) {
		// The shelf is followed, but its link did not answer: the page says which way.
		const code = (await res.json().catch(() => null))?.error as string | undefined;
		const list = await fetch('/api/linked-shelves');
		const shelves: LinkedShelf[] = list.ok ? await list.json() : [];
		const shelf = shelves.find((s) => String(s.id) === params.id);
		if (!shelf) error(404, t('shelf.notFound'));
		return { linked: { shelf, books: [] as LinkedBook[] }, failed: code ?? 'fetch failed' };
	}
	const linked: { shelf: LinkedShelf; books: LinkedBook[] } = await res.json();
	return { linked, failed: null as string | null };
};
