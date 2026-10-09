import type { Catalog, CatalogPage } from '#lib/types';
import type { PageLoad } from './$types';

// Where the reader is in a catalog lives in the URL (?c=<id>&url=<feed>, or
// &q=<terms> for a search), so the back button walks back through the
// catalog the way it was entered.
export const load: PageLoad = async ({ fetch, url, parent }) => {
	// Off for the instance or for this user: nothing to ask the server for.
	if (!(await parent()).catalogs) {
		return { catalogs: [] as Catalog[], selected: null, q: '', atStart: true, page: null, pageError: null, available: false };
	}
	const selected = Number(url.searchParams.get('c')) || null;
	const feedUrl = url.searchParams.get('url');
	const q = url.searchParams.get('q') ?? '';
	const params = new URLSearchParams();
	if (feedUrl) params.set('url', feedUrl);
	if (q) params.set('q', q);
	const [listRes, pageRes] = await Promise.all([
		fetch('/api/catalogs'),
		selected ? fetch(`/api/catalogs/${selected}/feed?${params}`) : Promise.resolve(null)
	]);
	const catalogs: Catalog[] = listRes.ok ? await listRes.json() : [];
	const page: CatalogPage | null = pageRes?.ok ? await pageRes.json() : null;
	// Why a page could not be shown: 'refused', 'missing', or anything else.
	const pageError: string | null =
		pageRes != null && !pageRes.ok ? ((await pageRes.json().catch(() => null))?.error ?? 'failed') : null;
	return { catalogs, selected, q, atStart: !feedUrl && !q, page, pageError, available: true };
};
