import { redirect } from '@sveltejs/kit';
import { setLocale } from '#lib/i18n';
import type { FedStatus, Shelf } from '#lib/types';
import type { LayoutLoad } from './$types';

export const ssr = false;
export const prerender = false;

export const load: LayoutLoad = async ({ fetch, url }) => {
	const res = await fetch('/api/auth/me');
	const user = res.ok ? await res.json() : null;
	setLocale(user?.locale ?? 'en');

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
	// Federation UI stays hidden unless the server says it is available.
	let fed: FedStatus = { available: false, mode: 'off', host: null };
	if (user) {
		const [shelvesRes, configRes, fedRes] = await Promise.all([
			fetch('/api/shelves'),
			fetch('/api/config'),
			fetch('/api/fed/status').catch(() => null)
		]);
		if (fedRes?.ok) {
			fed = await fedRes.json().catch(() => fed);
		}
		shelves = shelvesRes.ok ? await shelvesRes.json() : [];
		if (configRes.ok) {
			const config: {
				libris_enabled: boolean;
				openlibrary_enabled?: boolean;
				mcp_enabled?: boolean;
				audiobooks_enabled?: boolean;
			} = await configRes.json();
			audiobooksEnabled = config.audiobooks_enabled ?? false;
			mcpEnabled = config.mcp_enabled ?? false;
			librisEnabled = config.libris_enabled;
			openLibraryEnabled = config.openlibrary_enabled ?? false;
		}
	}

	return { user, shelves, librisEnabled, openLibraryEnabled, mcpEnabled, audiobooksEnabled, fed };
};
