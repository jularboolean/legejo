// A highlight put into the reader's own language by the librarian's model.
// The server keeps the translation with the highlight, so asking twice costs
// one request.

import { t } from '#lib/i18n';
import type { Annotation } from '#lib/types';

/** "en-GB" → "en". */
export function primaryLanguage(tag: string | null | undefined): string | null {
	const primary = tag?.split(/[-_]/)[0]?.trim().toLowerCase();
	return primary || null;
}

/** Whether a passage of a book in `bookLanguage` can be translated for a reader whose language is `locale`. */
export function canTranslate(bookLanguage: string | null | undefined, locale: string): boolean {
	return primaryLanguage(bookLanguage) !== primaryLanguage(locale);
}

/** The highlight with its translation; throws with the message to show. */
export async function translateAnnotation(id: number): Promise<Annotation> {
	let res: Response;
	try {
		res = await fetch(`/api/annotations/${id}/translate`, { method: 'POST' });
	} catch {
		throw new Error(t('common.network'));
	}
	if (res.ok) return res.json();
	const code = (await res.json().catch(() => null))?.error;
	if (code === 'too-many') throw new Error(t('reader.translate.tooMany'));
	if (code === 'same-language') throw new Error(t('reader.translate.same'));
	throw new Error(t('reader.translate.failed'));
}
