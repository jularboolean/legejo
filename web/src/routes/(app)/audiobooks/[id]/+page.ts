import { error } from '@sveltejs/kit';
import type { AudiobookDetail } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(`/api/audiobooks/${params.id}`);
	if (!res.ok) error(404, 'Not found');
	const audiobook: AudiobookDetail = await res.json();
	return { audiobook };
};
