import type { FedBook, FedFollow } from '#lib/types';

/** A shelf on another instance worth following (server: fed::remote::SUGGESTIONS). */
export type FedSuggestion = { handle: string; name: string; summary: string; language: string };
/** An instance an admin has connected, and the shelves it lists. */
export type FedConnected = {
	domain: string;
	shelves: { handle: string; name: string; summary: string; books: number }[];
};
import type { PageLoad } from './$types';

// The selected shelf lives in the URL (?actor=<IRI>), so the back button
// returns from a shelf's books to the list, which matters on a phone.
export const load: PageLoad = async ({ fetch, parent, url }) => {
	const { fed } = await parent();
	const actor = url.searchParams.get('actor');
	if (!fed.available) {
		return {
			follows: [] as FedFollow[],
			actor: null,
			books: null,
			booksFailed: false,
			suggestions: [] as FedSuggestion[],
			connected: Promise.resolve([] as FedConnected[])
		};
	}
	const [followsRes, booksRes, suggestionsRes] = await Promise.all([
		fetch('/api/fed/follows'),
		actor ? fetch(`/api/fed/books?actor=${encodeURIComponent(actor)}`) : Promise.resolve(null),
		fetch('/api/fed/suggestions')
	]);
	const suggestions: FedSuggestion[] = suggestionsRes.ok ? await suggestionsRes.json() : [];
	const follows: FedFollow[] = followsRes.ok ? await followsRes.json() : [];
	const books: FedBook[] | null = booksRes?.ok ? await booksRes.json() : null;
	// Not awaited: the other instances may take a moment to answer.
	const connected: Promise<FedConnected[]> = fetch('/api/fed/connected')
		.then((res) => (res.ok ? res.json() : []))
		.catch(() => []);
	return { follows, actor, books, booksFailed: booksRes != null && !booksRes.ok, suggestions, connected };
};
