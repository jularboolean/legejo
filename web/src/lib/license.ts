import { t, type MessageKey } from '#lib/i18n';

/** The fixed list in server/src/license.rs, in the order the picker shows. */
export const LICENSES = [
	'copyright',
	'pd',
	'cc0',
	'cc-by',
	'cc-by-sa',
	'cc-by-nc',
	'cc-by-nc-sa',
	'cc-by-nd',
	'cc-by-nc-nd'
] as const;

export type License = (typeof LICENSES)[number];

/** Why a book may not federate (license.rs NotFederable). */
export type NotFederable =
	| { code: 'unknown_license' }
	| { code: 'copyrighted' }
	| { code: 'missing_source' }
	| { code: 'missing_death_year' }
	| { code: 'still_protected'; died: number; free_from: number };

export type FederableStatus = { ok: true } | ({ ok: false } & NotFederable);

/** "CC BY-SA", "Public domain" … */
export function licenseName(license: string): string {
	if (license === 'pd') return t('license.pd');
	if (license === 'copyright') return t('license.copyright');
	if (license === 'cc0') return 'CC0';
	if (license.startsWith('cc-')) return 'CC ' + license.slice(3).toUpperCase();
	return license;
}

/** The CC deed for a licence, for a link on the book page. */
export function licenseUrl(license: string): string | null {
	if (license === 'cc0') return 'https://creativecommons.org/publicdomain/zero/1.0/';
	if (license.startsWith('cc-')) return `https://creativecommons.org/licenses/${license.slice(3)}/4.0/`;
	return null;
}

const REASON_KEYS: Record<NotFederable['code'], MessageKey> = {
	unknown_license: 'license.reason.unknown',
	copyrighted: 'license.reason.copyrighted',
	missing_source: 'license.reason.missingSource',
	missing_death_year: 'license.reason.missingDeathYear',
	still_protected: 'license.reason.stillProtected'
};

export function reasonText(reason: NotFederable): string {
	return reason.code === 'still_protected'
		? t(REASON_KEYS[reason.code], { died: reason.died, year: reason.free_from })
		: t(REASON_KEYS[reason.code]);
}

/**
 * The same rule as license::federable, for live feedback while editing.
 * The server decides; this only spares a round trip.
 */
export function federable(
	license: string,
	sourceUrl: string,
	deathYear: number | null,
	year = new Date().getFullYear()
): FederableStatus {
	if (!(LICENSES as readonly string[]).includes(license)) return { ok: false, code: 'unknown_license' };
	if (license === 'copyright') return { ok: false, code: 'copyrighted' };
	if (!/^https?:\/\/[^/?#\s]+\S*$/.test(sourceUrl.trim())) return { ok: false, code: 'missing_source' };
	if (license === 'pd') {
		if (deathYear == null) return { ok: false, code: 'missing_death_year' };
		if (year < deathYear + 71) {
			return { ok: false, code: 'still_protected', died: deathYear, free_from: deathYear + 71 };
		}
	}
	return { ok: true };
}
