import type { PageLoad } from './$types';

// The link from the mail sent to a new address: the address is used once
// it is opened, logged in or not.
export const load: PageLoad = async ({ fetch, url }) => {
	const token = url.searchParams.get('token') ?? '';
	if (!token) {
		return { ok: false, email: '' };
	}
	try {
		const res = await fetch('/api/account/email/confirm', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ token })
		});
		if (res.ok) {
			const body: { email: string } = await res.json();
			return { ok: true, email: body.email };
		}
	} catch {
		// fall through to the failure state
	}
	return { ok: false, email: '' };
};
