// The life years of authors, as far as the server has found them already
// (from Wikidata, when an admin has turned that on): fetched once per page
// load and shared by the pages that show many authors.

import { t } from '#lib/i18n';

export type Years = { born: number | null; died: number | null };

let asked: Promise<Record<string, Years>> | null = null;

/** Years by author key; empty when there are none or the server is out of reach. */
export function authorYears(): Promise<Record<string, Years>> {
	asked ??= fetch('/api/authors/years')
		.then((res) => (res.ok ? res.json() : {}))
		.catch(() => {
			asked = null;
			return {};
		});
	return asked;
}

const year = (y: number) => (y < 0 ? t('authors.bce', { year: -y }) : String(y));

/** "1858–1940", "born 1933", or null. */
export function yearsText(y: Years | undefined | null): string | null {
	if (!y || (y.born == null && y.died == null)) return null;
	if (y.born != null && y.died != null) return `${year(y.born)}–${year(y.died)}`;
	return y.born != null ? t('authors.born', { year: year(y.born) }) : t('authors.died', { year: year(y.died!) });
}
