import { redirect } from '@sveltejs/kit';
import type { LogPage } from '#lib/logText';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, url }) => {
	const params = new URLSearchParams();
	const action = url.searchParams.get('action');
	if (action) params.set('action', action);
	const res = await fetch(`/api/admin/log?${params}`);
	if (res.status === 403) redirect(307, '/');
	const page: LogPage = res.ok ? await res.json() : { entries: [], next: null };
	return { page, action: action ?? '' };
};
