import type { HandleClientError } from '@sveltejs/kit/hooks';
import { t } from '#lib/i18n';

// A page whose data could not be fetched at all (no network, or none worth
// the name) says so, instead of calling it an internal error.
export const handleError: HandleClientError = ({ error }) => {
	const unreachable =
		(typeof navigator !== 'undefined' && navigator.onLine === false) ||
		(error instanceof TypeError && /fetch|network|load failed/i.test(error.message));
	if (unreachable) return { message: t('offline.unreachable') };
};
