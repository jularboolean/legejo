import { listFrom } from '#lib/loaded';
import type { Note } from '#lib/notes';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/annotations');
	const notes = await listFrom<Note>(res);
	return { notes };
};
