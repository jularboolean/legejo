// What a comic book archive holds: its pages in reading order (as the server
// orders them), and from its ComicInfo.xml whether it is read from the right.
// A place in it is kept as `cbz:<page>` where an EPUB keeps its CFI.

import type { ZipEntry } from './zip';

export const CBZ_PREFIX = 'cbz:';

const IMAGE_TYPES: Record<string, string> = {
	jpg: 'image/jpeg',
	jpeg: 'image/jpeg',
	png: 'image/png',
	webp: 'image/webp',
	gif: 'image/gif',
	avif: 'image/avif'
};

export type ComicPage = { entry: ZipEntry; type: string };

/** Order names the way their pages are read: runs of digits by their value, so "page2" comes before "page10". */
export function naturalCompare(a: string, b: string): number {
	const split = (s: string) => s.toLowerCase().match(/\d+|\D+/g) ?? [];
	const x = split(a);
	const y = split(b);
	for (let i = 0; i < Math.min(x.length, y.length); i++) {
		const digits = /^\d/.test(x[i]) && /^\d/.test(y[i]);
		const diff = digits ? Number(x[i]) - Number(y[i]) : x[i] < y[i] ? -1 : x[i] > y[i] ? 1 : 0;
		if (diff !== 0) return diff;
	}
	return x.length - y.length;
}

/** The pages of an archive in reading order, and its ComicInfo.xml if it has one. */
export function comicContents(entries: ZipEntry[]): { pages: ComicPage[]; info: ZipEntry | null } {
	const pages: ComicPage[] = [];
	let info: ZipEntry | null = null;
	for (const entry of entries) {
		const base = entry.name.split('/').pop() ?? entry.name;
		// Folders of thumbnails and the like that archivers on macOS add.
		if (entry.name.startsWith('__MACOSX/') || base.startsWith('.') || entry.name.endsWith('/')) continue;
		if (base.toLowerCase() === 'comicinfo.xml') {
			info = entry;
			continue;
		}
		const type = IMAGE_TYPES[base.split('.').pop()?.toLowerCase() ?? ''];
		if (type) pages.push({ entry, type });
	}
	pages.sort((a, b) => naturalCompare(a.entry.name, b.entry.name));
	return { pages, info };
}

/** Whether a ComicInfo.xml says the pages are read from the right, as manga is. */
export function readsRightToLeft(xml: string): boolean {
	return /<Manga>\s*YesAndRightToLeft\s*<\/Manga>/i.test(xml);
}

export const encodePage = (page: number) => `${CBZ_PREFIX}${page}`;

/** The page of a `cbz:` place; null for anything else. */
export function pageOf(cfi: string | null | undefined): number | null {
	if (!cfi?.startsWith(CBZ_PREFIX)) return null;
	const n = Number(cfi.slice(CBZ_PREFIX.length));
	return Number.isInteger(n) && n >= 1 ? n : null;
}

/**
 * The pages shown together, as numbers from 1. One at a time, or as a
 * spread of two on a wide screen: the cover alone, as in a printed comic,
 * and a page that is itself wider than tall (a picture over two pages)
 * alone. `wide` says which pages are wider than tall, as far as known.
 */
export function spreads(total: number, twoUp: boolean, wide: (page: number) => boolean): number[][] {
	const out: number[][] = [];
	if (!twoUp) {
		for (let n = 1; n <= total; n++) out.push([n]);
		return out;
	}
	let n = 1;
	if (total > 0) out.push([n++]);
	while (n <= total) {
		if (wide(n) || n === total || wide(n + 1)) out.push([n++]);
		else {
			out.push([n, n + 1]);
			n += 2;
		}
	}
	return out;
}
