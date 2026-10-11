// Books saved on the device, for reading without the network. The page
// writes them into a cache of their own; the service worker answers the
// reader's requests from it when the network does not (service-worker.ts).

import type { BookDetail } from '#lib/types';
import { BOOKS_CACHE, DATA_CACHE, SESSION_PATHS, bookUrls } from './keys';

const VERSION_HEADER = 'x-legejo-version';
const SAVED_HEADER = 'x-legejo-saved';
const OWNER_KEY = 'legejo.offline.user';
/** Sent on the window when a book has been saved on or removed from the device. */
export const CHANGED_EVENT = 'legejo:saved-books';

function changed() {
	if (typeof window !== 'undefined') window.dispatchEvent(new Event(CHANGED_EVENT));
}

/** Opened from the home screen or the dock, as an app of its own. */
export function standalone(): boolean {
	if (typeof window === 'undefined') return false;
	return matchMedia('(display-mode: standalone)').matches || (navigator as { standalone?: boolean }).standalone === true;
}

/**
 * Whether books can be saved here:
 *  - `ready`: yes;
 *  - `install`: only once Legejo is on the home screen. Safari empties what a
 *    site in a tab has stored after a week unvisited; an installed app keeps it;
 *  - `unsupported`: the browser lacks what it takes (or the page is not served securely).
 */
export type Availability = 'ready' | 'install' | 'unsupported';

export function availability(): Availability {
	if (typeof window === 'undefined' || !('caches' in window) || !('serviceWorker' in navigator)) return 'unsupported';
	if (standalone()) return 'ready';
	const ua = navigator.userAgent;
	const ios = /iPad|iPhone|iPod/.test(ua) || (ua.includes('Macintosh') && navigator.maxTouchPoints > 1);
	const safari = /Safari\//.test(ua) && !/Chrome\/|Chromium\/|Edg\/|OPR\/|Firefox\//.test(ua);
	return ios || safari ? 'install' : 'ready';
}

/** What tells one state of a book's file from another. */
export function fileVersion(book: Pick<BookDetail, 'file_size' | 'updated_at' | 'created_at'>): string {
	return `${book.file_size}:${book.updated_at ?? book.created_at}`;
}

export type SavedBook = {
	book: BookDetail;
	/** Bytes of the file on the device. */
	size: number;
	/** When it was saved, in milliseconds. */
	savedAt: number;
	version: string;
};

async function books(): Promise<Cache> {
	return caches.open(BOOKS_CACHE);
}

/** The version of the file saved on the device, or null when the book is not saved. */
export async function savedVersion(id: number | string): Promise<string | null> {
	if (availability() === 'unsupported') return null;
	const file = await (await books()).match(bookUrls(id).file);
	return file ? (file.headers.get(VERSION_HEADER) ?? '') : null;
}

/** The books on the device, most recently saved first. */
export async function savedBooks(): Promise<SavedBook[]> {
	if (availability() === 'unsupported') return [];
	const cache = await books();
	const out: SavedBook[] = [];
	for (const request of await cache.keys()) {
		const match = /^\/api\/books\/(\d+)\/file$/.exec(new URL(request.url).pathname);
		if (!match) continue;
		const urls = bookUrls(match[1]);
		const [file, detail] = await Promise.all([cache.match(urls.file), cache.match(urls.detail)]);
		if (!file || !detail) continue;
		try {
			out.push({
				book: await detail.json(),
				size: Number(file.headers.get('content-length')) || 0,
				savedAt: Number(file.headers.get(SAVED_HEADER)) || 0,
				version: file.headers.get(VERSION_HEADER) ?? ''
			});
		} catch {
			// A detail that cannot be read: the book is listed again once saved anew.
		}
	}
	return out.sort((a, b) => b.savedAt - a.savedAt);
}

async function body(res: Response, onProgress?: (fraction: number) => void): Promise<Blob> {
	const total = Number(res.headers.get('content-length')) || 0;
	if (!onProgress || !total || !res.body) return res.blob();
	const reader = res.body.getReader();
	const chunks: BlobPart[] = [];
	let got = 0;
	for (;;) {
		const { done, value } = await reader.read();
		if (done) break;
		chunks.push(value);
		got += value.byteLength;
		onProgress(Math.min(1, got / total));
	}
	return new Blob(chunks, { type: res.headers.get('content-type') ?? 'application/epub+zip' });
}

/**
 * Save a book on the device: what the reader asks for, and the file last,
 * since the file is what makes it saved. `userId` is the one logged in, whose
 * books these are. Throws when the file cannot be
 * fetched or there is no room for it.
 */
export async function saveBook(book: BookDetail, userId: number, onProgress?: (fraction: number) => void): Promise<void> {
	const cache = await books();
	const urls = bookUrls(book.id);
	// Asked once, at the first book: without it the browser may empty the
	// cache when the device runs short of room. It may say no.
	navigator.storage?.persist?.().catch(() => {});
	rememberOwner(userId);

	const fileRes = await fetch(urls.file);
	if (!fileRes.ok) throw new Error(`the book file answered ${fileRes.status}`);
	const blob = await body(fileRes, onProgress);

	// The rest is best effort: the reader manages without any of it.
	const keep = async (url: string) => {
		try {
			const res = await fetch(url);
			if (res.status === 200) await cache.put(url, res);
		} catch {
			// Not there now; the service worker keeps it when it next comes by.
		}
	};
	await cache.put(urls.detail, new Response(JSON.stringify(book), { headers: { 'content-type': 'application/json' } }));
	await Promise.all([keep(urls.detail), keep(urls.progress), keep(urls.annotations), book.has_cover ? keep(urls.cover) : Promise.resolve()]);
	await warmSession();

	await cache.put(
		urls.file,
		new Response(blob, {
			headers: {
				'content-type': blob.type || 'application/epub+zip',
				'content-length': String(blob.size),
				[VERSION_HEADER]: fileVersion(book),
				[SAVED_HEADER]: String(Date.now())
			}
		})
	);
	changed();
}

export async function removeBook(id: number | string): Promise<void> {
	const cache = await books();
	const urls = bookUrls(id);
	// The file first: without it the book is no longer saved, whatever is left.
	await cache.delete(urls.file);
	await Promise.all([urls.detail, urls.cover, urls.progress, urls.annotations].map((url) => cache.delete(url)));
	changed();
}

/**
 * Ask for what every page needs, so that the service worker has it for when
 * the network is gone. Does nothing until the worker is in charge of the page.
 */
export async function warmSession(): Promise<void> {
	if (typeof navigator === 'undefined' || !navigator.serviceWorker?.controller) return;
	await Promise.all(SESSION_PATHS.map((path) => fetch(path).catch(() => null)));
}

/** Everything kept on the device for the one logged in: gone, as at logout. */
export async function forgetEverything(): Promise<void> {
	if (typeof window === 'undefined' || !('caches' in window)) return;
	await Promise.all([caches.delete(BOOKS_CACHE), caches.delete(DATA_CACHE)]);
	try {
		localStorage.removeItem(OWNER_KEY);
	} catch {
		// No storage: nothing to forget there.
	}
}

function rememberOwner(userId: number) {
	try {
		localStorage.setItem(OWNER_KEY, String(userId));
	} catch {
		// Without it the check below finds nothing to compare with.
	}
}

/** What one user saved is not left for the next one to log in on the device. */
export async function keepOnlyFor(userId: number): Promise<void> {
	let owner: string | null = null;
	try {
		owner = localStorage.getItem(OWNER_KEY);
	} catch {
		return;
	}
	if (owner !== null && owner !== String(userId)) await forgetEverything();
}

/** How much the device holds for Legejo and how much it allows, in bytes, when the browser says. */
export async function room(): Promise<{ used: number; quota: number } | null> {
	try {
		const estimate = await navigator.storage?.estimate?.();
		if (!estimate || !estimate.quota) return null;
		return { used: estimate.usage ?? 0, quota: estimate.quota };
	} catch {
		return null;
	}
}
