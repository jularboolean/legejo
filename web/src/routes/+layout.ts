import { error, redirect } from '@sveltejs/kit';
import { setLocale, t } from '#lib/i18n';
import { CACHED_HEADER } from '#lib/offline/keys';
import { keepOnlyFor } from '#lib/offline/store';
import type { FedStatus, Shelf } from '#lib/types';
import type { LayoutLoad } from './$types';

export const ssr = false;
export const prerender = false;

export const load: LayoutLoad = async ({ fetch, url }) => {
	let res: Response;
	try {
		res = await fetch('/api/auth/me');
	} catch {
		// No network, and nothing kept on the device to go by.
		error(503, t('offline.unreachable'));
	}
	const user = res.ok ? await res.json() : null;
	setLocale(user?.locale ?? 'en');
	// The answer came from the device's own cache (service-worker.ts): the
	// server cannot be reached. Only what is on the device can be shown.
	const offline = res.headers.get(CACHED_HEADER) === '1';
	if (offline && user && url.pathname !== '/offline' && !/^\/books\/\d+\/read$/.test(url.pathname)) {
		redirect(307, '/offline');
	}
	if (user && !offline) await keepOnlyFor(user.id).catch(() => {});

	const openPaths = ['/login', '/register', '/verify', '/forgot', '/reset', '/invite'];
	if (!user && !openPaths.includes(url.pathname)) {
		redirect(307, '/login');
	}
	if (user && url.pathname === '/login') {
		redirect(307, '/');
	}

	let shelves: Shelf[] = [];
	let librisEnabled = true;
	let openLibraryEnabled = false;
	let mcpEnabled = false;
	let audiobooksEnabled = false;
	let sendToKindle = false;
	let librarian = false;
	let catalogs = false;
	let catalogsAvailable = false;
	// Federation UI stays hidden unless the server says it is available.
	let fed: FedStatus = { available: false, enabled: false, mode: 'off', host: null };
	if (user) {
		const [shelvesRes, configRes, fedRes] = await Promise.all([
			fetch('/api/shelves').catch(() => null),
			fetch('/api/config').catch(() => null),
			fetch('/api/fed/status').catch(() => null)
		]);
		if (fedRes?.ok) {
			fed = await fedRes.json().catch(() => fed);
		}
		shelves = shelvesRes?.ok ? await shelvesRes.json() : [];
		if (configRes?.ok) {
			const config: {
				libris_enabled: boolean;
				openlibrary_enabled?: boolean;
				mcp_enabled?: boolean;
				audiobooks_enabled?: boolean;
				send_to_kindle?: boolean;
				librarian?: boolean;
				catalogs?: boolean;
				catalogs_available?: boolean;
			} = await configRes.json();
			audiobooksEnabled = config.audiobooks_enabled ?? false;
			sendToKindle = config.send_to_kindle ?? false;
			librarian = config.librarian ?? false;
			catalogs = config.catalogs ?? false;
			catalogsAvailable = config.catalogs_available ?? false;
			mcpEnabled = config.mcp_enabled ?? false;
			librisEnabled = config.libris_enabled;
			openLibraryEnabled = config.openlibrary_enabled ?? false;
		}
	}

	return { user, offline, shelves, librisEnabled, openLibraryEnabled, mcpEnabled, audiobooksEnabled, sendToKindle, librarian, catalogs, catalogsAvailable, fed };
};
