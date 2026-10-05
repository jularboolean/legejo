// Book dates on the web side, mirroring server/src/pubdate.rs: `published`
// (this edition) is "YYYY" or "YYYY-MM-DD"; `first_published` is a year.

/** "[2019]" → "2019", "2013-10-25T00:00…" → "2013-10-25"; null when unreadable. */
export function normalizePublished(raw: string | null | undefined): string | null {
	const s = raw?.trim() ?? '';
	const m = s.match(/^\D{0,5}?(\d{4})(?:-(\d{1,2})-(\d{1,2}))?/);
	if (!m) return null;
	const year = Number(m[1]);
	if (year < 1000 || year > new Date().getFullYear() + 1) return null;
	if (m[2] && m[3]) {
		const mo = Number(m[2]);
		const d = Number(m[3]);
		const date = new Date(Date.UTC(year, mo - 1, d));
		if (date.getUTCMonth() === mo - 1 && date.getUTCDate() === d) {
			return `${m[1]}-${String(mo).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
		}
	}
	return m[1];
}

/** "2013-10-25" → "25 okt. 2013" in the reader's language; "2013" stays. */
export function formatPublished(published: string, locale: string): string {
	if (!/^\d{4}-\d{2}-\d{2}$/.test(published)) return published;
	const [y, m, d] = published.split('-').map(Number);
	return new Intl.DateTimeFormat(locale === 'en' ? 'en-GB' : locale, { dateStyle: 'medium', timeZone: 'UTC' }).format(
		new Date(Date.UTC(y, m - 1, d))
	);
}

/** Negative years read as "800 f.Kr." etc. are left to the caller; plain integer check here. */
export function parseYear(raw: string): number | null | undefined {
	const s = raw.trim();
	if (s === '') return null;
	if (!/^-?\d{1,4}$/.test(s)) return undefined;
	const y = Number(s);
	return y >= -3000 && y <= new Date().getFullYear() + 1 ? y : undefined;
}
