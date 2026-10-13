import { error } from '@sveltejs/kit';
import { t } from '#lib/i18n';

/**
 * The list a page asked the server for. A failed request is an error to
 * show, never an empty list: a library that could not be fetched must not
 * look like a library without books.
 */
export async function listFrom<T>(res: Response): Promise<T[]> {
	if (!res.ok) error(res.status >= 500 ? 503 : res.status, t('net.failed'));
	return res.json();
}
