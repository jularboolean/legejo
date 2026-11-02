// Finding words in a page of a PDF. A page's text comes in pieces that may
// break anywhere, in the middle of a word too, so the words are looked for in
// the pieces put together, and a match is told as places in the pieces: the
// same text is laid in pieces over the page, where the match can be marked.

/** A piece of a page's text, and whether its line ends after it. */
export type Piece = { str: string; eol?: boolean };

/** A place in a piece, in UTF-16 units. */
export type PiecePlace = { piece: number; offset: number };

export type PageMatch = {
	from: PiecePlace;
	/** Just past the match. */
	to: PiecePlace;
	before: string;
	match: string;
	after: string;
};

/**
 * All the items of a page's text as pdf.js streams them. The stream is read by
 * hand, and not with `page.getTextContent()`: that loops over it with
 * `for await`, which Safari cannot do.
 */
export async function readItems<T>(stream: ReadableStream<{ items: T[] }>): Promise<T[]> {
	const reader = stream.getReader();
	const items: T[] = [];
	for (let part = await reader.read(); !part.done; part = await reader.read()) items.push(...part.value.items);
	return items;
}

/** Without case and accents, as the search compares. */
export function foldText(text: string): string {
	return text.toLocaleLowerCase().normalize('NFD').replace(/\p{M}/gu, '');
}

const SOFT_HYPHEN = '­';

/** Words as they are looked for: folded, with any white space a single space. */
export function foldWords(words: string): string {
	return foldText(words.replaceAll(SOFT_HYPHEN, '')).replace(/\s+/g, ' ').trim();
}

/** The next character that shows, from a place on. */
function nextShown(pieces: Piece[], piece: number, offset: number): string {
	for (let p = piece; p < pieces.length; p++) {
		for (const c of pieces[p].str.slice(p === piece ? offset : 0)) {
			if (c !== SOFT_HYPHEN && !/\s/.test(c)) return c;
		}
	}
	return '';
}

export function findInPage(pieces: Piece[], words: string): PageMatch[] {
	const needle = foldWords(words);
	if (!needle) return [];
	// The text, shown and folded, and where each folded unit came from.
	let shown = '';
	let folded = '';
	const from: { place: PiecePlace; end: PiecePlace; shown: number; shownEnd: number }[] = [];
	const add = (text: string, unit: string, piece: number, offset: number, length: number) => {
		const start = shown.length;
		shown += text;
		for (let i = 0; i < unit.length; i++) {
			folded += unit[i];
			from.push({ place: { piece, offset }, end: { piece, offset: offset + length }, shown: start, shownEnd: shown.length });
		}
	};
	pieces.forEach((piece, p) => {
		let offset = 0;
		for (const c of piece.str) {
			const at = offset;
			offset += c.length;
			if (c === SOFT_HYPHEN) continue;
			if (/\s/.test(c)) {
				if (shown && !shown.endsWith(' ')) add(' ', ' ', p, at, c.length);
				continue;
			}
			// A word split over a line end is one word.
			const rest = piece.eol && !piece.str.slice(offset).trim();
			if (rest && c === '-' && /\p{L}$/u.test(shown) && /^\p{Ll}/u.test(nextShown(pieces, p + 1, 0))) continue;
			add(c, foldText(c), p, at, c.length);
		}
		const end = piece.str.trimEnd();
		const split = end.endsWith(SOFT_HYPHEN) || end.endsWith('-');
		if (piece.eol && shown && !shown.endsWith(' ') && !split) add(' ', ' ', p, piece.str.length, 0);
	});

	const out: PageMatch[] = [];
	for (let at = folded.indexOf(needle); at >= 0; at = folded.indexOf(needle, at + needle.length)) {
		const first = from[at];
		const last = from[at + needle.length - 1];
		out.push({
			from: first.place,
			to: last.end,
			before: shown.slice(Math.max(0, first.shown - 40), first.shown),
			match: shown.slice(first.shown, last.shownEnd),
			after: shown.slice(last.shownEnd, last.shownEnd + 60)
		});
	}
	return out;
}
