import type { PublicShelf } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/public/shelves');
	const shelves: PublicShelf[] = res.ok ? await res.json() : [];
	return { publicShelves: shelves };
};
