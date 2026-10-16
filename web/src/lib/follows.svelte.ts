// What is new on the shared shelves the user follows. Fetched when the app
// opens and again after each page change, so the sidebar's count and the
// note in the library go away once a shelf has been looked at (opening a
// shelf's page is what marks it as seen, on the server).

import type { ShelfNews } from './types';

export const followNews = $state<{ shelves: ShelfNews[] }>({ shelves: [] });

/** Books not yet seen, over all followed shelves. */
export function newBookCount(): number {
	return followNews.shelves.reduce((sum, s) => sum + s.new_books, 0);
}

export async function refreshFollowNews(): Promise<void> {
	try {
		const res = await fetch('/api/follows/news');
		if (res.ok) followNews.shelves = await res.json();
	} catch {
		// Offline or the server is away: keep what was there.
	}
}

export async function markAllSeen(): Promise<void> {
	followNews.shelves = [];
	try {
		await fetch('/api/follows/seen', { method: 'POST' });
	} catch {
		// Shown again next time, which is no harm.
	}
}

export function shelfHref(owner: string, name: string): string {
	return `/public/${encodeURIComponent(owner)}/${encodeURIComponent(name)}`;
}
