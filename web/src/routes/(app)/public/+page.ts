import type { LinkedShelf, PublicShelf } from '#lib/types';
import { listFrom } from '#lib/loaded';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, parent }) => {
	const { shelfLinks } = await parent();
	const [res, linkedRes] = await Promise.all([
		fetch('/api/public/shelves'),
		// Shelves followed by a private link: only where an admin has turned that on.
		shelfLinks ? fetch('/api/linked-shelves') : Promise.resolve(null)
	]);
	const shelves = await listFrom<PublicShelf>(res);
	const linkedShelves: LinkedShelf[] = linkedRes?.ok ? await linkedRes.json() : [];
	return { publicShelves: shelves, linkedShelves };
};
