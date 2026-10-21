import type { FedBook, FedFollow } from '#lib/types';

/** A shelf on another instance worth following (server: fed::remote::SUGGESTIONS). */
export type FedSuggestion = { handle: string; name: string; summary: string; language: string };
import type { PageLoad } from './$types';

// The selected shelf lives in the URL (?actor=<IRI>), so the back button
// returns from a shelf's books to the list, which matters on a phone.
export const load: PageLoad = async ({ fetch, parent, url }) => {
	const { fed } = await parent();
	const actor = url.searchParams.get('actor');
	if (!fed.available) {
		return { follows: [] as FedFollow[], actor: null, books: null, booksFailed: false, suggestions: [] as FedSuggestion[] };
	}
	const [followsRes, booksRes, suggestionsRes] = await Promise.all([
		fetch('/api/fed/follows'),
		actor ? fetch(`/api/fed/books?actor=${encodeURIComponent(actor)}`) : Promise.resolve(null),
		fetch('/api/fed/suggestions')
	]);
	const suggestions: FedSuggestion[] = suggestionsRes.ok ? await suggestionsRes.json() : [];
	const follows: FedFollow[] = followsRes.ok ? await followsRes.json() : [];
	const books: FedBook[] | null = booksRes?.ok ? await booksRes.json() : null;
	return { follows, actor, books, booksFailed: booksRes != null && !booksRes.ok, suggestions };
};
