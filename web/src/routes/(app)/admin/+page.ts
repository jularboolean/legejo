import { redirect } from '@sveltejs/kit';
import type { AdminUserRow, FedAdminSettings, FedInstance, FedOverview } from '#lib/types';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const [settingsRes, usersRes, fedRes, instancesRes, overviewRes] = await Promise.all([
		fetch('/api/admin/settings'),
		fetch('/api/admin/users'),
		fetch('/api/admin/federation'),
		fetch('/api/admin/federation/instances'),
		fetch('/api/admin/federation/overview')
	]);
	// Non-admins get 403; send them home rather than showing an error page.
	if (settingsRes.status === 403 || usersRes.status === 403) {
		redirect(307, '/');
	}
	const settings: {
		libris_enabled: boolean;
		openlibrary_enabled?: boolean;
		audiobooks_enabled?: boolean;
		registration_enabled: boolean;
		mail_configured: boolean;
	} = settingsRes.ok
		? await settingsRes.json()
		: { libris_enabled: true, registration_enabled: false, mail_configured: false };
	const users: AdminUserRow[] = usersRes.ok ? await usersRes.json() : [];
	// Federation: null when the server has no federation API (older build).
	const federation: FedAdminSettings | null = fedRes.ok ? await fedRes.json() : null;
	const instances: FedInstance[] = instancesRes.ok ? await instancesRes.json() : [];
	const overview: FedOverview | null = overviewRes.ok ? await overviewRes.json() : null;
	return { settings, users, federation, instances, overview };
};
