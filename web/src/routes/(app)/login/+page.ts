import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	let registrationEnabled = false;
	let mailConfigured = false;
	let oidcName: string | null = null;
	// A new instance with no account: the first visitor creates the admin.
	let setup = false;
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
		if (res.ok) {
			const body = await res.json();
			oidcName = body.oidc?.name ?? null;
			setup = body.setup === true;
		}
	} catch {
		// Without it, the password form is the way in.
	}
	return { registrationEnabled, mailConfigured, oidcName, setup };
};
