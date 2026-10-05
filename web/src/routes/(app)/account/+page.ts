import { error } from '@sveltejs/kit';
import type { Account } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/account');
	if (!res.ok) {
		error(500);
	}
	const account: Account = await res.json();
	return { account };
};
