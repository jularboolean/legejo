import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, url }) => {
	const token = url.searchParams.get('token') ?? '';
	if (!token) {
		return { ok: false, username: '' };
	}
	try {
		const res = await fetch('/api/register/verify', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ token })
		});
		if (res.ok) {
			const body: { username: string } = await res.json();
			return { ok: true, username: body.username };
		}
	} catch {
		// fall through to the failure state
	}
	return { ok: false, username: '' };
};
