import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	let registrationEnabled = false;
	let mailConfigured = false;
	let oidcName: string | null = null;
	try {
		const res = await fetch('/api/register/enabled');
		if (res.ok) {
			const body = await res.json();
			registrationEnabled = body.enabled;
			mailConfigured = body.mail_configured;
		}
	} catch {
		// The links are conveniences; login works without them.
	}
	try {
		const res = await fetch('/api/auth/config');
		if (res.ok) oidcName = (await res.json()).oidc?.name ?? null;
	} catch {
		// Without it, the password form is the way in.
	}
	return { registrationEnabled, mailConfigured, oidcName };
};
