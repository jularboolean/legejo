// A highlight as a picture to post: the passage set in the book face beside
// the cover, with the title and author under it. Drawn in the browser on a
// canvas; the server is not asked for anything but the cover.

/** Longer passages are cut: a card is for a quotation, not a page. */
export const MAX_QUOTE_CHARS = 500;

export const CARD_WIDTH = 1200;
const MIN_HEIGHT = 675;
const MAX_HEIGHT = 1500;
const PAD = 80;
const COVER_WIDTH = 300;
const GAP = 70;
/** Quote sizes to try, largest first, until the passage fits. */
const QUOTE_SIZES = [54, 48, 44, 40, 36, 32, 29, 26];
const LINE = 1.45;
const TITLE_LINE = 54;
/** The strip at the foot for the doves. */
const FOOT = 70;
const SHADOW_HEIGHT = 68;
/** How much of the ink the doves' shadow has: to be noticed second, if at all. */
const SHADOW_INK = 0.16;
/** The doves' width over their height (assets/legejo.svg). */
const LOGO_RATIO = 321.825 / 341.68;

const BOOK_FACE = '"Literata Variable", Georgia, serif';
const TITLE_FACE = 'Fraunces, Georgia, serif';

/** The colours of the card: the design chosen in the app, or these. */
export type Palette = { paper: string; ink: string; muted: string; mark: string };

export const DEFAULT_PALETTE: Palette = { paper: '#f6f2ea', ink: '#1f2325', muted: '#6c7477', mark: '#c19a4b' };

/** The app's colours as the page shows them now: --card, --fg, --muted and --gold. */
export function pageColors(): Palette {
	if (typeof document === 'undefined') return DEFAULT_PALETTE;
	const style = getComputedStyle(document.documentElement);
	const take = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
	return {
		paper: take('--card', DEFAULT_PALETTE.paper),
		ink: take('--fg', DEFAULT_PALETTE.ink),
		muted: take('--muted', DEFAULT_PALETTE.muted),
		mark: take('--gold', DEFAULT_PALETTE.mark)
	};
}

export type CardInput = {
	quote: string;
	title: string;
	author: string | null;
	cover: CanvasImageSource | null;
	/** The cover's width over its height. */
	coverRatio?: number;
	/** The highlight's colour, for the bar beside the passage. */
	color?: string | null;
	/** The two doves, a shadow at the foot. */
	logo?: CanvasImageSource | null;
	palette?: Palette;
};

/** The passage as it goes on the card: whitespace made even, and cut at a word when too long. */
export function cardQuote(text: string, max = MAX_QUOTE_CHARS): string {
	const even = text.replace(/\s+/g, ' ').trim();
	if (even.length <= max) return even;
	const cut = even.slice(0, max);
	const space = cut.lastIndexOf(' ');
	return `${(space > max * 0.6 ? cut.slice(0, space) : cut).replace(/[\s,;:.–—-]+$/, '')}…`;
}

/** Break text into lines no wider than `width`, as measured. A word wider than a line stands alone. */
export function wrap(text: string, width: number, measure: (s: string) => number): string[] {
	const lines: string[] = [];
	let line = '';
	for (const word of text.split(' ')) {
		const next = line ? `${line} ${word}` : word;
		if (line && measure(next) > width) {
			lines.push(line);
			line = word;
		} else line = next;
	}
	if (line) lines.push(line);
	return lines;
}

/** The largest size at which the passage fits in `height`, and its lines; the smallest size if none does. */
export function fit(
	text: string,
	width: number,
	height: number,
	measureAt: (size: number) => (s: string) => number
): { size: number; lines: string[] } {
	let best = { size: QUOTE_SIZES[QUOTE_SIZES.length - 1], lines: [] as string[] };
	for (const size of QUOTE_SIZES) {
		const lines = wrap(text, width, measureAt(size));
		best = { size, lines };
		if (lines.length * size * LINE <= height) break;
	}
	return best;
}

/** A colour safe to paint with: a hex colour as given, else the default. */
function barColor(color: string | null | undefined, fallback: string): string {
	return color && /^#[0-9a-f]{3,8}$/i.test(color) ? color : fallback;
}

/** What `work` comes to, or `fallback` if it has not come to anything in time. */
export function within<T>(work: Promise<T>, ms: number, fallback: T): Promise<T> {
	return Promise.race([work, new Promise<T>((resolve) => setTimeout(() => resolve(fallback), ms))]);
}

/** Make sure the faces the card is set in are loaded, so the canvas does not fall back. */
export async function loadFaces(text: string): Promise<void> {
	if (typeof document === 'undefined' || !document.fonts) return;
	await Promise.all([
		document.fonts.load(`400 40px ${BOOK_FACE}`, text),
		document.fonts.load(`italic 400 40px ${BOOK_FACE}`, text),
		document.fonts.load(`600 40px ${TITLE_FACE}`)
	]).catch(() => {});
}

/** Draw the card; the canvas is sized to fit the passage. */
export function drawCard(canvas: HTMLCanvasElement, input: CardInput): void {
	const ctx = canvas.getContext('2d');
	if (!ctx) return;
	const quote = cardQuote(input.quote);
	const colors = input.palette ?? DEFAULT_PALETTE;
	const textX = input.cover ? PAD + COVER_WIDTH + GAP : PAD;
	const textWidth = CARD_WIDTH - textX - PAD;
	const coverHeight = input.cover ? COVER_WIDTH / (input.coverRatio || 2 / 3) : 0;

	const measureAt = (size: number) => {
		ctx.font = `400 ${size}px ${BOOK_FACE}`;
		return (s: string) => ctx.measureText(s).width;
	};
	// Room for the passage: what is left of the tallest card after the
	// quotation mark above it and the book's name below.
	const markRoom = 70;
	const footRoom = 190;
	const { size, lines } = fit(quote, textWidth - 30, MAX_HEIGHT - 2 * PAD - FOOT - markRoom - footRoom, measureAt);
	const quoteHeight = lines.length * size * LINE;

	ctx.font = `600 40px ${TITLE_FACE}`;
	const titleLines = wrap(input.title, textWidth, (s) => ctx.measureText(s).width).slice(0, 2);
	const textHeight = markRoom + quoteHeight + 60 + titleLines.length * TITLE_LINE + (input.author ? 50 : 0);
	const height = Math.round(Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, textHeight + 2 * PAD + FOOT, coverHeight + 2 * PAD + FOOT)));

	canvas.width = CARD_WIDTH;
	canvas.height = height;
	ctx.fillStyle = colors.paper;
	ctx.fillRect(0, 0, CARD_WIDTH, height);

	if (input.cover) {
		const y = PAD;
		ctx.save();
		ctx.shadowColor = 'rgba(30, 25, 15, 0.28)';
		ctx.shadowBlur = 30;
		ctx.shadowOffsetY = 10;
		ctx.fillStyle = colors.paper;
		ctx.fillRect(PAD, y, COVER_WIDTH, coverHeight);
		ctx.restore();
		ctx.drawImage(input.cover, PAD, y, COVER_WIDTH, coverHeight);
	}

	// The passage sits in the middle of the room it has, the book under it.
	let y = PAD + Math.max(0, (height - 2 * PAD - FOOT - textHeight) / 2);
	ctx.fillStyle = barColor(input.color, colors.mark);
	ctx.font = `600 120px ${TITLE_FACE}`;
	ctx.textBaseline = 'alphabetic';
	ctx.fillText('“', textX - 6, y + 90);
	y += markRoom;

	ctx.fillRect(textX, y + 6, 5, quoteHeight - size * (LINE - 1));
	ctx.fillStyle = colors.ink;
	ctx.font = `400 ${size}px ${BOOK_FACE}`;
	ctx.textBaseline = 'top';
	for (const line of lines) {
		ctx.fillText(line, textX + 30, y + (size * (LINE - 1)) / 2);
		y += size * LINE;
	}

	y += 60;
	ctx.fillStyle = colors.ink;
	ctx.font = `600 40px ${TITLE_FACE}`;
	for (const line of titleLines) {
		ctx.fillText(line, textX, y);
		y += TITLE_LINE;
	}
	if (input.author) {
		ctx.fillStyle = colors.muted;
		ctx.font = `italic 400 32px ${BOOK_FACE}`;
		ctx.fillText(input.author, textX, y + 8, textWidth);
	}

	// The doves, at the foot on the right: a shadow, without a colour of their own.
	if (input.logo) {
		const shadow = silhouette(input.logo, SHADOW_HEIGHT, colors.ink);
		ctx.globalAlpha = SHADOW_INK;
		ctx.drawImage(shadow, CARD_WIDTH - PAD - shadow.width, height - PAD / 2 - shadow.height);
		ctx.globalAlpha = 1;
	}
}

/** The picture's outline, filled with one colour. */
function silhouette(logo: CanvasImageSource, height: number, ink: string): HTMLCanvasElement {
	const shape = document.createElement('canvas');
	shape.width = Math.round(height * LOGO_RATIO);
	shape.height = Math.round(height);
	const ctx = shape.getContext('2d');
	if (!ctx) return shape;
	ctx.drawImage(logo, 0, 0, shape.width, shape.height);
	ctx.globalCompositeOperation = 'source-in';
	ctx.fillStyle = ink;
	ctx.fillRect(0, 0, shape.width, shape.height);
	return shape;
}

/** The words that go with the picture: the sharer's own, then where the passage is from. */
export function postText(comment: string, from: string): string {
	const own = comment.trim();
	return own ? `${own}\n\n${from}` : from;
}

/** The picture described, for those who cannot see it. */
export function altText(quote: string, title: string, author: string | null): string {
	return `“${cardQuote(quote)}” — ${title}${author ? `, ${author}` : ''}`;
}

/** A name for the file: the title in plain letters. */
export function fileName(title: string): string {
	const plain = title
		.normalize('NFKD')
		.replace(/[̀-ͯ]/g, '')
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-|-$/g, '')
		.slice(0, 50);
	return `${plain || 'quote'}-quote.png`;
}
