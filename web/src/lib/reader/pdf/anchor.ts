// Where a place or a passage is in a PDF, as the string the server keeps in
// the `cfi` field of a position or a highlight (it holds an EPUB CFI for an
// EPUB). The page comes first, so that highlights sort in reading order with
// `compareCfi` (lib/notes.ts), which compares the numbers in the string.
//
//   position:  pdf:12
//   passage:   pdf:12:0.1032,0.0812,0.0221,0.7351/0.1253,0.0812,0.0221,0.5|13:…
//
// A passage is one or more pages, each with rectangles as fractions of the
// page: top, left, height and width, four decimals.

export const PDF_PREFIX = 'pdf:';

/** A rectangle on a page, as fractions of its width and height. */
export type PdfRect = { top: number; left: number; height: number; width: number };
/** A passage: the rectangles it covers, page by page (pages counted from 1). */
export type PdfPassage = { page: number; rects: PdfRect[] }[];

const round = (n: number) => Math.round(Math.min(1, Math.max(0, n)) * 10_000) / 10_000;

export function isPdfPlace(cfi: string | null | undefined): boolean {
	return !!cfi && cfi.startsWith(PDF_PREFIX);
}

/** The position string for a page. */
export function encodePage(page: number): string {
	return `${PDF_PREFIX}${Math.max(1, Math.round(page))}`;
}

/** The page a position or a passage starts on; null for anything else. */
export function pageOf(cfi: string | null | undefined): number | null {
	if (!isPdfPlace(cfi)) return null;
	const page = Number.parseInt(cfi!.slice(PDF_PREFIX.length), 10);
	return Number.isFinite(page) && page >= 1 ? page : null;
}

export function encodePassage(passage: PdfPassage): string {
	return (
		PDF_PREFIX +
		passage
			.filter((p) => p.rects.length > 0)
			.map((p) => `${p.page}:` + p.rects.map((r) => [r.top, r.left, r.height, r.width].map(round).join(',')).join('/'))
			.join('|')
	);
}

/** The passage a highlight covers; null for a position, an EPUB CFI or anything malformed. */
export function decodePassage(cfi: string | null | undefined): PdfPassage | null {
	if (!isPdfPlace(cfi)) return null;
	const passage: PdfPassage = [];
	for (const part of cfi!.slice(PDF_PREFIX.length).split('|')) {
		const [pageText, rectsText] = part.split(':');
		const page = Number.parseInt(pageText, 10);
		if (!Number.isFinite(page) || page < 1 || !rectsText) return null;
		const rects: PdfRect[] = [];
		for (const r of rectsText.split('/')) {
			const n = r.split(',').map(Number);
			if (n.length !== 4 || n.some((x) => !Number.isFinite(x))) return null;
			rects.push({ top: n[0], left: n[1], height: n[2], width: n[3] });
		}
		passage.push({ page, rects });
	}
	return passage.length > 0 ? passage : null;
}

/**
 * The rectangles of a selection made into one per line: a browser gives one
 * per run of text, often overlapping, and a line is what a highlight shows.
 * Rectangles count as one line when their middles are less than half a line
 * apart. In reading order: top to bottom, then left to right.
 */
export function mergeLines(rects: PdfRect[]): PdfRect[] {
	const sorted = rects.filter((r) => r.width > 0 && r.height > 0).sort((a, b) => a.top - b.top || a.left - b.left);
	const lines: PdfRect[] = [];
	for (const r of sorted) {
		const line = lines.find((l) => Math.abs(l.top + l.height / 2 - (r.top + r.height / 2)) < Math.min(l.height, r.height) / 2);
		if (!line) {
			lines.push({ ...r });
			continue;
		}
		const top = Math.min(line.top, r.top);
		const left = Math.min(line.left, r.left);
		const bottom = Math.max(line.top + line.height, r.top + r.height);
		const right = Math.max(line.left + line.width, r.left + r.width);
		Object.assign(line, { top, left, height: bottom - top, width: right - left });
	}
	return lines.sort((a, b) => a.top - b.top || a.left - b.left);
}
