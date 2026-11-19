import { listFrom } from '#lib/loaded';
import type { Note, TrashedNote } from '#lib/notes';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const [res, trashRes] = await Promise.all([fetch('/api/annotations'), fetch('/api/annotations/trash').catch(() => null)]);
	const notes = await listFrom<Note>(res);
	const trash: TrashedNote[] = trashRes?.ok ? await trashRes.json().catch(() => []) : [];
	return { notes, trash };
};
