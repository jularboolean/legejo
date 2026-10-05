import type { FedBook, FedFollow } from '#lib/types';
import type { PageLoad } from './$types';

// The selected shelf lives in the URL (?actor=<IRI>), so the back button
// returns from a shelf's books to the list, which matters on a phone.
export const load: PageLoad = async ({ fetch, parent, url }) => {
	const { fed } = await parent();
	const actor = url.searchParams.get('actor');
	if (!fed.available) {
		return { follows: [] as FedFollow[], actor: null, books: null, booksFailed: false };
	}
	const [followsRes, booksRes] = await Promise.all([
		fetch('/api/fed/follows'),
		actor ? fetch(`/api/fed/books?actor=${encodeURIComponent(actor)}`) : Promise.resolve(null)
	]);
	const follows: FedFollow[] = followsRes.ok ? await followsRes.json() : [];
	const books: FedBook[] | null = booksRes?.ok ? await booksRes.json() : null;
	return { follows, actor, books, booksFailed: booksRes != null && !booksRes.ok };
};
