/// <reference no-default-lib="true"/>
/// <reference lib="esnext" />
/// <reference lib="webworker" />

// What makes Legejo open, and a saved book readable, without the network.
//
// Three caches:
//  - the shell: the app's own files for this version, taken at install;
//  - session data: who is logged in and what the menus need, kept as last seen;
//  - books: what the page has put there for the books saved on the device
//    (lib/offline/store.ts). This worker only reads it and keeps it fresh.
//
// The network always comes first for anything that can change. The caches
// answer when it does not, and say so with a header the app looks for.

import { assets, immutable } from '$app/manifest';
import { BOOKS_CACHE, CACHED_HEADER, DATA_CACHE, SESSION_PATHS, bookKey } from '../lib/offline/keys';

const sw = self as unknown as ServiceWorkerGlobalScope;

/** The app's own files: the built ones and those of the static folder, as paths from the root. */
const rooted = (entry: string | { path: string }) => {
	const path = typeof entry === 'string' ? entry : entry.path;
	return path.startsWith('/') ? path : `/${path}`;
};
const ASSETS = new Set([...immutable, ...assets].map(rooted).filter((path) => path !== '/service-worker.js'));

// One cache per version of the app. The built files carry a hash of their
// content in their names, so the list of them tells one version from another.
const SHELL_PREFIX = 'legejo-shell-';
const SHELL_CACHE = `${SHELL_PREFIX}${fingerprint([...ASSETS].sort().join('\n'))}`;

function fingerprint(text: string): string {
	let h = 5381;
	for (let i = 0; i < text.length; i++) h = ((h << 5) + h + text.charCodeAt(i)) | 0;
	return (h >>> 0).toString(36);
}
/** How long the network gets before a cached answer is used instead. */
const PATIENCE_MS = 4000;
/**
 * How long it gets to begin answering the page's own requests for data at
 * all. Longer than the server itself waits for another library's catalog
 * (fifteen seconds), so that a slow one of those is not cut short here.
 */
const GIVE_UP_MS = 30000;

sw.addEventListener('install', (event) => {
	event.waitUntil(
		(async () => {
			const cache = await caches.open(SHELL_CACHE);
			await cache.addAll([...ASSETS]);
			// The page itself. During a rolling deploy it can come from another
			// version than this worker: then its files are not the ones just
			// cached, and installing waits for the next try.
			const page = await fetch('/', { cache: 'no-store' });
			if (!page.ok) throw new Error(`the app page answered ${page.status}`);
			const html = await page.clone().text();
			const wanted = html.match(/\/_app\/immutable\/[^"'\s)]+/g) ?? [];
			if (wanted.some((path) => !ASSETS.has(path))) throw new Error('the app page is of another version');
			await cache.put('/', page);
			await sw.skipWaiting();
		})()
	);
});

sw.addEventListener('activate', (event) => {
	event.waitUntil(
		(async () => {
			for (const name of await caches.keys()) {
				if (name.startsWith(SHELL_PREFIX) && name !== SHELL_CACHE) await caches.delete(name);
			}
			await sw.clients.claim();
		})()
	);
});

/** A cached answer, marked as such. */
function marked(response: Response): Response {
	const headers = new Headers(response.headers);
	headers.set(CACHED_HEADER, '1');
	return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

/**
 * The network's answer, kept under `key` when it is a good one; the cached
 * answer when the network fails or takes too long. Without a cached answer
 * the network's failure is the answer.
 */
async function networkFirst(request: Request, cacheName: string, key: string, keep = true): Promise<Response> {
	const cache = await caches.open(cacheName);
	const cached = await cache.match(key);
	const fresh = fetch(request).then(async (response) => {
		// Kept only while it is still wanted: a book may have been removed meanwhile.
		if (keep && response.status === 200 && (cacheName !== BOOKS_CACHE || (await cache.match(key)))) {
			await cache.put(key, response.clone());
		}
		return response;
	});
	if (!cached) return fresh;
	const late = new Promise<null>((resolve) => setTimeout(() => resolve(null), PATIENCE_MS));
	try {
		const first = await Promise.race([fresh, late]);
		if (first) return first;
	} catch {
		return marked(cached);
	}
	fresh.catch(() => {});
	return marked(cached);
}

/**
 * The network's answer, or a failure once it has kept the page waiting too
 * long to begin one: a network that is there in name only otherwise leaves
 * the app hanging, with nothing to tell its user. Only the wait for the
 * answer to begin is timed; a long download goes on for as long as it takes.
 */
async function withinReason(request: Request): Promise<Response> {
	const fresh = fetch(request);
	const late = new Promise<null>((resolve) => setTimeout(() => resolve(null), GIVE_UP_MS));
	const first = await Promise.race([fresh, late]);
	if (first) return first;
	fresh.catch(() => {});
	return Response.error();
}

async function cacheFirst(request: Request, cacheName: string, key: string, keep: boolean): Promise<Response> {
	const cache = await caches.open(cacheName);
	const cached = await cache.match(key);
	if (cached) return cached;
	const response = await fetch(request);
	if (keep && response.status === 200) await cache.put(key, response.clone());
	return response;
}

async function navigation(request: Request): Promise<Response> {
	const shell = async () => (await caches.open(SHELL_CACHE)).match('/');
	const late = new Promise<null>((resolve) => setTimeout(() => resolve(null), PATIENCE_MS));
	const fresh = fetch(request);
	try {
		const first = await Promise.race([fresh, late]);
		if (first) return first;
	} catch {
		const page = await shell();
		if (page) return page;
		throw new Error('offline, and the app is not on this device yet');
	}
	fresh.catch(() => {});
	return (await shell()) ?? fresh;
}

sw.addEventListener('fetch', (event) => {
	const request = event.request;
	if (request.method !== 'GET') return;
	const url = new URL(request.url);
	if (url.origin !== sw.location.origin) return;
	const path = url.pathname;

	if (path.startsWith('/api/')) {
		if (SESSION_PATHS.includes(path)) {
			event.respondWith(
				networkFirst(request, DATA_CACHE, path).then(async (response) => {
					// The server says nobody is logged in: what was kept of the
					// session goes, so the app does not open as that user again
					// without the network. Saved books stay for the next login
					// (the page removes them if it is someone else's).
					if (path === '/api/auth/me' && response.status === 401) await caches.delete(DATA_CACHE);
					return response;
				})
			);
			return;
		}
		const key = bookKey(path);
		if (!key) {
			// What the page fetches for itself; pictures, sound and downloads
			// are left to the browser.
			if (request.destination === '' && !request.headers.has('range')) event.respondWith(withinReason(request));
			return;
		}
		event.respondWith(
			(async () => {
				const saved = await (await caches.open(BOOKS_CACHE)).match(key.url);
				if (!saved) return request.destination === '' ? withinReason(request) : fetch(request);
				// The file is what it was when saved; the page saves it anew
				// when the book has changed.
				if (key.kind === 'file') return cacheFirst(request, BOOKS_CACHE, key.url, false);
				// A thumbnail is answered by the saved cover, never kept as it.
				return networkFirst(request, BOOKS_CACHE, key.url, !url.searchParams.has('size'));
			})()
		);
		return;
	}
	if (path.startsWith('/podcast/') || path.startsWith('/ap/') || path === '/_app/version.json') return;

	if (request.mode === 'navigate') {
		event.respondWith(navigation(request));
		return;
	}
	if (ASSETS.has(path)) {
		event.respondWith(cacheFirst(request, SHELL_CACHE, path, false));
		return;
	}
	if (path.startsWith('/_app/immutable/')) {
		event.respondWith(cacheFirst(request, SHELL_CACHE, path, true));
	}
});
