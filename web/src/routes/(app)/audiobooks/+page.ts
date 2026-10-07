import { redirect } from '@sveltejs/kit';
import type { Audiobook } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/audiobooks');
	// Not turned on for this instance.
	if (res.status === 404) redirect(307, '/');
	const audiobooks: Audiobook[] = res.ok ? await res.json() : [];
	return { audiobooks };
};
