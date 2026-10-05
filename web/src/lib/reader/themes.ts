import type { ReaderFont, ReaderSettings, ReaderTheme, ThemePalette } from './types';

export const palettes: Record<ReaderTheme, ThemePalette> = {
	light: {
		bg: '#fcfdfd',
		fg: '#1d2224',
		muted: '#6c7a7e',
		link: '#53676c',
		surface: '#ffffff',
		border: '#dbe1e2'
	},
	sepia: {
		bg: '#f4ecd8',
		fg: '#4a3b28',
		muted: '#8a7860',
		link: '#96481f',
		surface: '#faf4e4',
		border: '#dccfb2'
	},
	dark: {
		bg: '#15181a',
		fg: '#d2d8da',
		muted: '#8d9a9e',
		link: '#94b0b8',
		surface: '#1f2426',
		border: '#333a3d'
	}
};

export const fontStacks: Record<Exclude<ReaderFont, 'book'>, string> = {
	serif: "'Iowan Old Style', 'Palatino Linotype', Palatino, Georgia, 'Times New Roman', serif",
	sans: "system-ui, -apple-system, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif"
};

/** Attribute the engine puts on text blocks that the alignment setting may change. */
export const ALIGN_MARK = 'data-legejo-align';

export type ContentCssContext = {
	/** Height of one page in px; images are scaled to fit inside it. */
	pageHeight: number;
	/** Side margin in px (scrolled flow only; paginated flow uses the column gap). */
	marginPx: number;
	/** Widest text column in px (scrolled flow only). */
	columnPx: number;
};

/**
 * The stylesheet injected into every chapter document. Book stylesheets are
 * arbitrary, so the rules that must win are marked !important.
 */
export function contentCss(settings: ReaderSettings, ctx: ContentCssContext): string {
	const p = palettes[settings.theme];
	const rules: string[] = [];

	rules.push(`html { font-size: ${settings.fontSize}% !important; }`);
	rules.push(
		`html, body { background: ${p.bg} !important; color: ${p.fg} !important; }`,
		`body { line-height: ${settings.lineHeight} !important; overflow-wrap: break-word; -webkit-text-size-adjust: 100%; text-size-adjust: 100%; }`,
		`p, li, blockquote, dd, dt, td, th { line-height: inherit !important; }`,
		`a, a:link, a:visited { color: ${p.link} !important; }`,
		`::selection { background: ${p.link}55; }`
	);

	// Every theme recolours the text uniformly: books often set color on some
	// elements but not others, and a partial override reads as two text colours
	// mixed on the same page (e.g. the book's #000 next to the theme's warm fg).
	rules.push(
		`body *:not(a):not(img):not(svg):not(svg *):not(video) { color: inherit !important; background-color: transparent !important; border-color: ${p.border} !important; }`,
		`hr { border-color: ${p.muted} !important; }`
	);
	if (settings.theme === 'dark') {
		// Scans on a white background would glare, so pictures are dimmed a
		// little. Drawings with black ink on a transparent background (formulas,
		// ornaments, diagrams) would vanish against the dark page; they get the
		// paper they were drawn for.
		rules.push(
			`img, svg, video { filter: brightness(0.86); }`,
			`img, svg:not(:has(image)) { background-color: #e9e5dc !important; }`,
			`svg:not(:has(image)) { color: #211f1c !important; }`
		);
	}

	if (settings.fontFamily !== 'book') {
		const stack = fontStacks[settings.fontFamily];
		rules.push(
			`body, p, li, blockquote, dd, dt, td, th, div, span, a, em, i, b, strong, h1, h2, h3, h4, h5, h6 { font-family: ${stack} !important; }`
		);
	}

	if (settings.textAlign !== 'book') {
		// Only blocks the book itself sets flush left or justified carry the mark,
		// so centred headings, verse and right-aligned datelines stay as they are.
		const value = settings.textAlign === 'justify' ? 'justify' : 'start';
		rules.push(`[${ALIGN_MARK}] { text-align: ${value} !important; }`);
	}
	if (settings.hyphenate) {
		rules.push(
			`body { -webkit-hyphens: auto !important; hyphens: auto !important; }`,
			`p, li, blockquote, dd, td { -webkit-hyphens: auto !important; hyphens: auto !important; }`,
			`h1, h2, h3, h4, h5, h6, pre, code { -webkit-hyphens: manual !important; hyphens: manual !important; }`
		);
	}

	// Images never exceed one page, in either direction.
	const maxImage = Math.max(120, Math.floor(ctx.pageHeight) - 48);
	rules.push(
		`img, svg, video, canvas { max-width: 100% !important; max-height: ${maxImage}px !important; object-fit: contain; box-sizing: border-box; }`,
		`img, video { height: auto; }`,
		`img, svg, figure, table { break-inside: avoid; page-break-inside: avoid; }`,
		`pre { white-space: pre-wrap !important; }`,
		`table { max-width: 100% !important; }`
	);

	if (settings.flow === 'scrolled') {
		rules.push(
			`body { max-width: ${Math.round(ctx.columnPx + 2 * ctx.marginPx)}px !important; margin: 0 auto !important; padding: 24px ${Math.round(ctx.marginPx)}px 0 !important; box-sizing: border-box !important; }`
		);
	}

	return rules.join('\n');
}
