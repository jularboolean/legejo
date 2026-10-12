import { error } from '@sveltejs/kit';
import type { Account, KoboDevices } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/account');
	if (!res.ok) {
		error(500);
	}
	const account: Account = await res.json();
	// The Kobo devices seen and how the last sync went: only with a token to sync with.
	let koboDevices: KoboDevices | null = null;
	if (account.kobo_token) {
		const devices = await fetch('/api/account/kobo/devices').catch(() => null);
		if (devices?.ok) koboDevices = await devices.json();
	}
	return { account, koboDevices };
};
