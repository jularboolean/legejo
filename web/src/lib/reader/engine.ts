// The only module that talks to epub.js. It opens a book, renders it into a
// host element and exposes navigation, location and settings through the
// `ReaderEngine` interface in ./types.

import ePub from 'epubjs';
import type { Book, Contents, Location, NavItem, Rendition } from 'epubjs';
import { ALIGN_MARK, contentCss, palettes } from './themes';
import {
	ReaderError,
	type Mark,
	type Passage,
	type ReaderEngine,
	type ReaderEngineOptions,
	type ReaderLocation,
	type ReaderSettings,
	type SearchHit,
	type SearchOptions,
	type SpeechText,
	type TocItem
} from './types';

/**
 * Width of the text column in em of the reader's text size. Lines are never
 * longer than the maximum (some 65–80 characters), and two pages are shown side
 * by side only when both get at least the minimum.
 */
const MEASURE_MAX_EM = 33;
const MEASURE_MIN_EM = 24;
/** Side margins stay within these, in px, whatever the setting. */
const MARGIN_MIN_PX = 10;
const MARGIN_MAX_PX = 72;
/** A search stops after this many hits. */
const SEARCH_MAX_HITS = 300;
const SEARCH_CONTEXT_CHARS = 64;
/** Characters per generated location; ~1 location per page of text. */
const LOCATION_CHARS = 1600;
const OPEN_TIMEOUT_MS = 30_000;
const DISPLAY_TIMEOUT_MS = 20_000;
const SETTLE_TIMEOUT_MS = 2_500;
/** Empty space after the last line of a chapter in scrolled flow, in px. */
const SCROLL_END_PAD = 96;
/** An element positioned this far from where it stands is meant to be out of sight, in px. */
const OFFSCREEN_PX = 1000;
const STYLE_KEY = 'legejo-reader';
const LOCATIONS_PREFIX = 'legejo.reader.locations.';

type FlatToc = {
	item: TocItem;
	/** Spine index of the chapter file, or -1 when the href can't be resolved. */
	index: number;
	fragment: string | null;
	/** Overall position (0–1) of the entry's anchor; only for files shared by several entries. */
	start?: number;
};

// epub.js keeps a few things we need off its public typings.
type SpineSection = {
	index: number;
	href: string;
	/** Path of the chapter file inside the archive, with a leading slash. */
	url: string;
	load(request: unknown): Promise<Element>;
	unload(): void;
	cfiFromElement(el: Element): string;
	cfiFromRange(range: Range): string;
	/** The parsed chapter while it is loaded. */
	contents?: Element;
};
type InternalSpine = { spineItems: SpineSection[]; get(target: string): SpineSection | null };
type InternalRendition = Rendition & {
	manager?: { container?: HTMLElement };
};

function timeout<T>(promise: Promise<T>, ms: number): Promise<T> {
	return new Promise<T>((resolve, reject) => {
		const timer = setTimeout(() => reject(new Error('timeout')), ms);
		promise.then(
			(value) => {
				clearTimeout(timer);
				resolve(value);
			},
			(err) => {
				clearTimeout(timer);
				reject(err);
			}
		);
	});
}

function clamp01(n: number): number {
	return Math.min(1, Math.max(0, n));
}

/** Read a response body, reporting progress when the server says how long it is. */
async function readBody(res: Response, onProgress?: (fraction: number) => void): Promise<ArrayBuffer> {
	const total = Number(res.headers.get('Content-Length'));
	// A compressed response reports its compressed length, which says nothing
	// about the number of bytes that come out of the stream.
	const measurable = total > 0 && !res.headers.get('Content-Encoding');
	if (!onProgress || !measurable || !res.body) return res.arrayBuffer();

	const reader = res.body.getReader();
	const chunks: Uint8Array[] = [];
	let received = 0;
	for (;;) {
		const { done, value } = await reader.read();
		if (done) break;
		chunks.push(value);
		received += value.byteLength;
		onProgress(Math.min(1, received / total));
	}
	const out = new Uint8Array(received);
	let offset = 0;
	for (const chunk of chunks) {
		out.set(chunk, offset);
		offset += chunk.byteLength;
	}
	return out.buffer;
}

async function download(url: string, onProgress?: (fraction: number) => void): Promise<ArrayBuffer> {
	let res: Response;
	try {
		res = await fetch(url);
	} catch {
		throw new ReaderError('network');
	}
	if (res.status === 401) throw new ReaderError('unauthorized');
	if (res.status === 404) throw new ReaderError('not-found');
	if (!res.ok) throw new ReaderError('network');

	let buffer: ArrayBuffer;
	try {
		buffer = await readBody(res, onProgress);
	} catch {
		throw new ReaderError('network');
	}
	// An EPUB is a zip archive; anything else can't be opened.
	const magic = new Uint8Array(buffer.slice(0, 2));
	if (magic[0] !== 0x50 || magic[1] !== 0x4b) throw new ReaderError('invalid');
	return buffer;
}

function readCachedLocations(key: string, version: string): string | null {
	try {
		const raw = localStorage.getItem(LOCATIONS_PREFIX + key);
		if (!raw) return null;
		const cached = JSON.parse(raw);
		return cached?.version === version && typeof cached.locations === 'string'
			? cached.locations
			: null;
	} catch {
		return null;
	}
}

function writeCachedLocations(key: string, version: string, locations: string) {
	const name = LOCATIONS_PREFIX + key;
	const value = JSON.stringify({ version, locations });
	try {
		localStorage.setItem(name, value);
		return;
	} catch {
		// Probably full: the locations of other books are the only big entries
		// the reader keeps, and they can be generated again. Drop them, try once more.
	}
	try {
		for (let i = localStorage.length - 1; i >= 0; i--) {
			const other = localStorage.key(i);
			if (other?.startsWith(LOCATIONS_PREFIX) && other !== name) localStorage.removeItem(other);
		}
		localStorage.setItem(name, value);
	} catch {
		// Storage unavailable: locations are regenerated next time.
	}
}

function escapeRegExp(text: string): string {
	return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Elements that don't break the line; text on either side of them reads as one run. */
const INLINE_TAGS = new Set(
	'a abbr b bdi bdo big cite code dfn em font i ins kbd mark q s samp small span strike strong sub sup tt u var'.split(
		' '
	)
);

/** How an element is hidden where it would otherwise be moved out of sight. */
const CLIPPED: [string, string][] = [
	// On the first page and inside its text area, clear of the side margin
	// (at most MARGIN_MAX_PX): a chapter is measured by the extent of its
	// content, and a box in the margin would add to it and cost a page.
	['left', `${MARGIN_MAX_PX + 8}px`],
	['right', 'auto'],
	['top', '32px'],
	['bottom', 'auto'],
	['width', '1px'],
	['height', '1px'],
	['margin', '0'],
	['overflow', 'hidden'],
	['clip-path', 'inset(50%)'],
	['white-space', 'nowrap']
];

/** Whether a CSS offset puts an element far outside the page. */
function farOff(value: string): boolean {
	const m = /^(-?[\d.]+)(px|pt|em|rem|ex|ch|%)?$/.exec(value.trim());
	if (!m) return false;
	const n = Math.abs(parseFloat(m[1]));
	const unit = m[2] ?? 'px';
	if (unit === 'px' || unit === 'pt') return n >= OFFSCREEN_PX;
	if (unit === '%') return n >= 300;
	return n >= OFFSCREEN_PX / 16;
}

/**
 * Books hide text meant for screen readers by positioning it far outside the
 * page (`position: absolute; left: -999em`). A chapter is laid out in columns
 * as wide as the page, and such an element stretches it by dozens of empty
 * pages. This reads the book's stylesheets and returns rules that hide those
 * elements by clipping instead. They go into the stylesheet every chapter
 * gets, so they hold before the first layout.
 */
async function offscreenRules(files: Record<string, unknown>): Promise<string> {
	if (typeof CSSStyleSheet !== 'function' || !('replaceSync' in CSSStyleSheet.prototype)) return '';
	const selectors: string[] = [];
	const collect = (rules: CSSRuleList) => {
		for (const rule of rules) {
			if ('cssRules' in rule && !(rule instanceof CSSStyleRule)) {
				collect((rule as CSSGroupingRule).cssRules);
				continue;
			}
			if (!(rule instanceof CSSStyleRule)) continue;
			const style = rule.style;
			if (style.position !== 'absolute' && style.position !== 'fixed') continue;
			if (![style.left, style.right, style.top, style.bottom].some(farOff)) continue;
			// A selector with a namespace prefix means nothing outside its own sheet.
			if (!rule.selectorText.includes('|')) selectors.push(rule.selectorText);
		}
	};
	for (const [name, entry] of Object.entries(files)) {
		if (!name.toLowerCase().endsWith('.css')) continue;
		try {
			const text = await (entry as { async(type: 'string'): Promise<string> }).async('string');
			const sheet = new CSSStyleSheet();
			sheet.replaceSync(text);
			collect(sheet.cssRules);
		} catch {
			// A stylesheet that can't be read or parsed has nothing to tell.
		}
	}
	const body = CLIPPED.map(([name, value]) => `${name}: ${value} !important;`).join(' ');
	return selectors.map((selector) => `\n${selector} { ${body} }`).join('');
}

/** Elements whose text is not read aloud. */
const UNSPOKEN_TAGS = new Set(['script', 'style', 'rt', 'rp', 'head', 'title', 'svg', 'math']);

/** Whether a text node is left out of reading aloud: hidden, or a note marker. */
function unspoken(node: Node, body: Element): boolean {
	for (let el = node.parentElement; el && el !== body; el = el.parentElement) {
		if (UNSPOKEN_TAGS.has(el.localName)) return true;
		if (el.hasAttribute('hidden') || el.getAttribute('aria-hidden') === 'true') return true;
		const type = `${el.getAttribute('epub:type') ?? ''} ${el.getAttribute('role') ?? ''}`;
		if (/\b(noteref|doc-noteref|pagebreak|doc-pagebreak)\b/.test(type)) return true;
	}
	return false;
}

/** Split a paragraph into sentences, as offsets into it. */
function sentenceSpans(text: string, language: string): [number, number][] {
	const spans: [number, number][] = [];
	const Segmenter = (Intl as unknown as { Segmenter?: typeof Intl.Segmenter }).Segmenter;
	if (Segmenter) {
		let segmenter: Intl.Segmenter;
		try {
			segmenter = new Segmenter(language || undefined, { granularity: 'sentence' });
		} catch {
			segmenter = new Segmenter(undefined, { granularity: 'sentence' });
		}
		for (const part of segmenter.segment(text)) spans.push([part.index, part.index + part.segment.length]);
	} else {
		// Without a segmenter: end a sentence at . ! ? … followed by space.
		const end = /[.!?…]+["'”’»)\]]*\s+/g;
		let from = 0;
		for (let m = end.exec(text); m; m = end.exec(text)) {
			spans.push([from, m.index + m[0].length]);
			from = m.index + m[0].length;
		}
		if (from < text.length) spans.push([from, text.length]);
	}
	return spans;
}

function blockOf(node: Node): Node | null {
	let el = node.parentNode;
	while (el && el.nodeType === 1 && INLINE_TAGS.has((el as Element).localName)) el = el.parentNode;
	return el;
}

/**
 * Download and open an EPUB and show it in `options.host`.
 * Rejects with a `ReaderError` when the book can't be fetched or parsed.
 */
export async function openReader(options: ReaderEngineOptions): Promise<ReaderEngine> {
	const { host } = options;
	const buffer = await download(options.url, options.onDownload);

	const book: Book = ePub(buffer);
	try {
		await timeout(book.ready, OPEN_TIMEOUT_MS);
	} catch {
		try {
			book.destroy();
		} catch {
			// Already half-open; nothing more to clean up.
		}
		throw new ReaderError('invalid');
	}

	const spine = book.spine as unknown as InternalSpine;
	const sections = spine.spineItems ?? [];
	if (sections.length === 0) {
		book.destroy();
		throw new ReaderError('invalid');
	}

	const packageMeta = (book as unknown as { packaging?: { metadata?: Record<string, string> } })
		.packaging?.metadata;
	const rtl = packageMeta?.direction === 'rtl';
	const metadata = {
		title: packageMeta?.title ?? '',
		author: packageMeta?.creator ?? '',
		language: packageMeta?.language ?? '',
		rtl
	};

	// Until the locations are worked out, progress is estimated from where the
	// chapter sits in the book. Chapters differ wildly in length, so they are
	// weighted by file size (from the zip directory) rather than counted.
	const weightBefore: number[] = [];
	let weightTotal = 0;
	{
		const files =
			(book as unknown as { archive?: { zip?: { files?: Record<string, unknown> } } }).archive?.zip
				?.files ?? {};
		const sizeOf = (section: SpineSection): number => {
			const path = (section.url ?? '').replace(/^\//, '');
			let entry = files[path] as { _data?: { uncompressedSize?: number } } | undefined;
			if (!entry) {
				try {
					entry = files[decodeURIComponent(path)] as typeof entry;
				} catch {
					// Malformed escape; no size for this one.
				}
			}
			const size = entry?._data?.uncompressedSize;
			return typeof size === 'number' && size > 0 ? size : 0;
		};
		const sizes = sections.map(sizeOf);
		// All or nothing: a mix of real sizes and guesses would be worse than counting.
		const known = sizes.every((size) => size > 0);
		for (const size of sizes) {
			weightBefore.push(weightTotal);
			weightTotal += known ? size : 1;
		}
	}

	/** Estimated overall position (0–1) of a point `within` (0–1) the chapter at `index`. */
	function estimate(index: number, within: number): number {
		const at = sections.findIndex((section) => section.index === index);
		if (at < 0 || weightTotal === 0) return 0;
		const next = at + 1 < sections.length ? weightBefore[at + 1] : weightTotal;
		return (weightBefore[at] + (next - weightBefore[at]) * within) / weightTotal;
	}

	// ---- Table of contents ------------------------------------------------

	/** TOC hrefs are relative to the nav file; map them onto spine items. */
	function resolveHref(href: string): { href: string; index: number; fragment: string | null } {
		const [rawPath, fragment = null] = href.split('#');
		let path = rawPath;
		let section = spine.get(path);
		if (!section) {
			try {
				path = decodeURIComponent(rawPath);
				section = spine.get(path);
			} catch {
				// Malformed escape; keep the raw path.
			}
		}
		if (!section) {
			const clean = path.replace(/^(\.\.?\/)+/, '');
			section =
				sections.find(
					(s) => s.href === clean || s.href.endsWith('/' + clean) || clean.endsWith('/' + s.href)
				) ?? null;
		}
		if (!section) return { href, index: -1, fragment };
		return {
			href: section.href + (fragment ? '#' + fragment : ''),
			index: section.index,
			fragment
		};
	}

	const flatToc: FlatToc[] = [];
	function buildToc(items: NavItem[], depth: number, prefix: string): TocItem[] {
		return items.map((nav, i) => {
			const id = prefix + i;
			const resolved = resolveHref(nav.href ?? '');
			const item: TocItem = {
				id,
				label: (nav.label ?? '').trim() || '—',
				href: resolved.href,
				depth,
				children: []
			};
			flatToc.push({ item, index: resolved.index, fragment: resolved.fragment });
			item.children = buildToc(nav.subitems ?? [], depth + 1, id + '.');
			return item;
		});
	}
	const toc = buildToc(book.navigation?.toc ?? [], 0, '');

	// ---- State ------------------------------------------------------------

	let settings: ReaderSettings = { ...options.settings };
	let rendition: InternalRendition | null = null;
	let current: ReaderLocation | null = null;
	let locationsReady = false;
	let destroyed = false;
	let paginated = settings.flow === 'paginated';
	let size = { width: 0, height: 0 };
	// The position the reader chose. Re-renders (resize, new settings) show it
	// again without adopting the start of whatever page it lands on, or the
	// position would creep backwards with every re-layout.
	let anchor: string | null = options.initialCfi ?? null;
	let layoutPass = true;
	let layoutTimer: ReturnType<typeof setTimeout> | null = null;
	let css = '';
	const offscreenCss = await offscreenRules(
		(book as unknown as { archive?: { zip?: { files?: Record<string, unknown> } } }).archive?.zip?.files ?? {}
	);
	let cssContext = { pageHeight: 0, marginPx: 0, columnPx: 0 };
	// Renders run one at a time; a resize during a re-render waits its turn.
	let queue: Promise<void> = Promise.resolve();

	const viewport = document.createElement('div');
	viewport.style.margin = '0 auto';
	viewport.style.height = '100%';
	host.appendChild(viewport);

	const reducedMotion = () =>
		typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

	const scroller = (): HTMLElement | null =>
		(!paginated && rendition?.manager?.container) || null;

	// The reader's stylesheet has to be in the chapter before epub.js paginates
	// it: styles added afterwards (from a rendition hook) reflow the text once
	// the page for a CFI has already been picked, landing a page or more off.
	// So it is written into the serialized chapter itself.
	//
	// Hooks all get the chapter as it was first serialized and pass their result
	// on through `section.output`. epub.js's own hook has by now put blob URLs
	// in place of the book's images, stylesheets and fonts there, so this one
	// must build on `section.output`: starting over from `output` would bring
	// back the relative URLs, which lead nowhere (the book is read from memory).
	const styleTag = () => `<style id="epubjs-inserted-css-${STYLE_KEY}">${css}</style>`;
	(
		book.spine as unknown as {
			hooks: { serialize: { register(fn: (output: string, section: { output: string }) => void): void } };
		}
	).hooks.serialize.register((output, section) => {
		let html = section.output ?? output;
		// A chapter may reload itself to another address after a delay. Nothing
		// in a book has a reason to, and it would load a page of the catalog
		// (or anything else) into the reading surface.
		html = html.replace(/<meta\b[^>]*http-equiv\s*=\s*["']?refresh[^>]*>/gi, '');
		// Chapters are XHTML but get parsed as HTML, where xml:lang means nothing.
		// Hyphenation and quotes depend on the language, so carry it over (or
		// fall back to the language of the book).
		html = html.replace(/<html\b[^>]*>/i, (tag) => {
			if (/\slang\s*=/i.test(tag)) return tag;
			const lang = /\sxml:lang\s*=\s*["']([\w-]+)["']/i.exec(tag)?.[1] ?? metadata.language;
			return /^[\w-]+$/.test(lang) ? tag.replace(/^<html\b/i, `<html lang="${lang}"`) : tag;
		});
		section.output = html;
		if (/<\/head>/i.test(html)) {
			section.output = html.replace(/<\/head>/i, () => styleTag() + '</head>');
		} else if (/<head\s*\/>/i.test(html)) {
			section.output = html.replace(/<head\s*\/>/i, () => `<head>${styleTag()}</head>`);
		}
	});

	// ---- Locations --------------------------------------------------------

	const cached = readCachedLocations(options.cacheKey, options.cacheVersion);
	if (cached) {
		try {
			book.locations.load(cached);
			locationsReady = book.locations.length() > 0;
		} catch {
			locationsReady = false;
		}
	}

	// epub.js idles 100 ms between chapters by default, which adds up to tens of
	// seconds for books split into hundreds of files.
	(book.locations as unknown as { pause: number }).pause = 8;

	const locationsDone: Promise<void> = locationsReady
		? Promise.resolve()
		: book.locations
				.generate(LOCATION_CHARS)
				.then(() => {
					if (destroyed) return;
					locationsReady = book.locations.length() > 0;
					if (!locationsReady) return;
					writeCachedLocations(options.cacheKey, options.cacheVersion, book.locations.save());
				})
				.catch(() => {
					// Without locations the reader still works; percentages stay estimates.
				});

	/**
	 * Where TOC entries start when several of them point into one file (books
	 * delivered as a single long chapter). Lets `chapterAt` tell them apart.
	 */
	async function indexAnchors() {
		const shared = new Map<number, FlatToc[]>();
		for (const entry of flatToc) {
			if (entry.index < 0) continue;
			const list = shared.get(entry.index);
			if (list) list.push(entry);
			else shared.set(entry.index, [entry]);
		}
		for (const [index, entries] of shared) {
			if (entries.length < 2 || destroyed) continue;
			const section = sections.find((s) => s.index === index);
			if (!section) continue;
			const wasLoaded = !!section.contents;
			try {
				const root = await section.load(book.load.bind(book));
				for (const entry of entries) {
					const el = entry.fragment
						? root.ownerDocument.getElementById(entry.fragment)
						: null;
					if (!el) continue;
					const at = book.locations.percentageFromCfi(section.cfiFromElement(el));
					if (Number.isFinite(at)) entry.start = at;
				}
			} catch {
				// The chapter can't be parsed; its entries stay indistinguishable.
			} finally {
				if (!wasLoaded) section.unload();
			}
		}
	}

	// ---- Location reporting -----------------------------------------------

	function tocEntryFor(loc: Location): TocItem | null {
		const index = loc.start.index;
		let best: FlatToc | null = null;
		let doc: Document | null = null;
		let visibleEnd: Range | null = null;
		let lookedUp = false;

		for (const entry of flatToc) {
			if (entry.index < 0 || entry.index > index) continue;
			if (entry.index < index || !entry.fragment) {
				best = entry;
				continue;
			}
			// Several entries point into this chapter: one counts once its anchor
			// has come into view.
			if (!lookedUp) {
				lookedUp = true;
				try {
					const contents = (rendition?.getContents() as unknown as Contents[]) ?? [];
					doc = contents.find((c) => c.sectionIndex === index)?.document ?? null;
					visibleEnd = rendition?.getRange(loc.end.cfi) ?? null;
				} catch {
					doc = null;
				}
			}
			const anchor = doc?.getElementById(entry.fragment) ?? null;
			if (!anchor || !visibleEnd) {
				if (!best || best.index < index) best = entry;
				continue;
			}
			try {
				if (visibleEnd.comparePoint(anchor, 0) <= 0) best = entry;
			} catch {
				// Anchor outside the range's document; ignore.
			}
		}
		return best?.item ?? null;
	}

	function toLocation(loc: Location): ReaderLocation {
		const start = loc.start;
		const page = start.displayed?.page ?? 1;
		const total = Math.max(1, start.displayed?.total ?? 1);

		let percent = NaN;
		let exact = false;
		if (locationsReady) {
			percent = loc.atEnd ? 1 : book.locations.percentageFromCfi(start.cfi);
			exact = Number.isFinite(percent);
		}
		if (!exact) {
			percent = loc.atEnd ? 1 : estimate(start.index, (page - 1) / total);
		}

		const lastShown = loc.end.displayed?.page ?? page;
		let atChapterEnd = lastShown >= total;
		const el = scroller();
		if (el) {
			atChapterEnd = el.scrollTop + el.clientHeight >= el.scrollHeight - SCROLL_END_PAD - 32;
		}

		const entry = tocEntryFor(loc);
		return {
			cfi: start.cfi,
			percent: clamp01(percent),
			exact,
			tocId: entry?.id ?? null,
			chapterLabel: entry?.label ?? null,
			page,
			pagesInChapter: total,
			pagesLeft: Math.max(0, total - lastShown),
			atChapterEnd,
			atStart: !!loc.atStart,
			atEnd: !!loc.atEnd
		};
	}

	function onRelocated(source: Rendition, loc: Location) {
		if (destroyed || source !== rendition || !loc?.start) return;
		const next = toLocation(loc);
		if (layoutPass && anchor) next.cfi = anchor;
		else anchor = next.cfi;
		current = next;
		options.onRelocated?.(next);
	}

	// ---- Rendering --------------------------------------------------------

	function measure() {
		// The content box: padding on the host is space the page must not use.
		const style = getComputedStyle(host);
		const px = (value: string) => parseFloat(value) || 0;
		const rect = host.getBoundingClientRect();
		return {
			width: Math.floor(rect.width - px(style.paddingLeft) - px(style.paddingRight)),
			height: Math.floor(rect.height - px(style.paddingTop) - px(style.paddingBottom))
		};
	}

	function teardown() {
		const old = rendition;
		rendition = null;
		if (!old) return;
		try {
			old.destroy();
		} catch {
			// epub.js can throw when destroyed mid-render; the DOM is cleared below.
		}
		viewport.replaceChildren();
	}

	function build() {
		teardown();
		size = measure();
		paginated = settings.flow === 'paginated';

		// Page geometry. The text column follows the text size, so lines keep a
		// readable length; what is left of a wide window becomes margin.
		const fontPx = (16 * settings.fontSize) / 100;
		const columnMax = Math.round(MEASURE_MAX_EM * fontPx);
		const columnMin = Math.round(MEASURE_MIN_EM * fontPx);
		const share = settings.margin / 100;
		const marginFor = (pageWidth: number) =>
			Math.min(MARGIN_MAX_PX, Math.max(MARGIN_MIN_PX, Math.round(pageWidth * share)));

		const spreadMargin = marginFor(size.width / 2);
		const spread =
			paginated &&
			settings.spread === 'auto' &&
			size.width > size.height * 1.15 &&
			size.width >= 2 * columnMin + 4 * spreadMargin;
		// One page: side margin, column, side margin. A spread adds the gutter,
		// which epub.js makes twice the side margin.
		const marginPx = spread
			? spreadMargin
			: marginFor(Math.min(size.width, columnMax / (1 - 2 * share)));
		let width = size.width;
		if (paginated) {
			width = Math.min(size.width, spread ? 2 * columnMax + 4 * marginPx : columnMax + 2 * marginPx);
			// Two columns of whole pixels, or the pages drift apart as they turn.
			if (spread) width -= width % 2;
		}

		viewport.style.width = width + 'px';
		host.style.background = palettes[settings.theme].bg;
		cssContext = { pageHeight: size.height, marginPx, columnPx: columnMax };
		css = contentCss(settings, cssContext) + offscreenCss;

		const created = book.renderTo(viewport, {
			width,
			height: size.height,
			flow: paginated ? 'paginated' : 'scrolled-doc',
			manager: 'default',
			spread: spread ? 'auto' : 'none',
			minSpreadWidth: 1,
			// The gap doubles as the side margin: half of it pads each page edge.
			...(paginated ? { gap: marginPx * 2 } : {}),
			resizeOnOrientationChange: false,
			// Book scripts would run with the catalog's origin and session.
			allowScriptedContent: false
		} as Parameters<Book['renderTo']>[1]) as InternalRendition;

		created.hooks.content.register((contents: Contents) => {
			// Normally a no-op (see the serialize hook); covers chapters without a <head>.
			contents.addStylesheetCss(css, STYLE_KEY);
			keepOnPage(contents.document);
			markAlignable(contents.document);
			options.onContent?.(contents.document);
		});
		created.on('relocated', (loc: Location) => onRelocated(created, loc));
		// The view has its own copy of the chapter by now; the parsed source and
		// its serialization would otherwise stay in memory for every chapter read.
		created.on('rendered', (section: SpineSection) => {
			try {
				section.unload();
			} catch {
				// Nothing to release.
			}
		});
		rendition = created;
		if (highlighted) mark(highlighted);
		for (const m of marks) drawMark(m);
	}

	/**
	 * The same as `offscreenRules`, for what the stylesheets did not show:
	 * rules in a chapter's own <style>, or inline. This runs after the first
	 * layout, so it shortens the chapter but may leave a blank page behind.
	 */
	function keepOnPage(doc: Document) {
		const view = doc.defaultView;
		if (!view || !doc.body) return;
		const far = (value: string) => Math.abs(parseFloat(value)) >= OFFSCREEN_PX;
		for (const el of doc.body.querySelectorAll<HTMLElement>('*')) {
			const style = view.getComputedStyle(el);
			if (style.position !== 'absolute' && style.position !== 'fixed') continue;
			if (!far(style.left) && !far(style.right) && !far(style.top) && !far(style.bottom)) continue;
			for (const [name, value] of CLIPPED) el.style.setProperty(name, value, 'important');
		}
	}

	/**
	 * Mark the text blocks that the book sets flush left or justified; the
	 * alignment setting applies to those only. Alignment doesn't move line
	 * breaks, so doing this after pagination is safe.
	 */
	function markAlignable(doc: Document) {
		const view = doc.defaultView;
		if (!view) return;
		for (const el of doc.querySelectorAll('p, li, dd, blockquote, div, td')) {
			const align = view.getComputedStyle(el).textAlign;
			if (align === 'center' || align === 'right' || align === 'end' || align === '-webkit-center') {
				continue;
			}
			el.setAttribute(ALIGN_MARK, '');
		}
	}

	// ---- Highlights and notes -----------------------------------------------

	/** What is drawn: the reader's highlights, by id. */
	let marks: Mark[] = [];

	function markStyle() {
		return {
			fill: '#f2c94c',
			'fill-opacity': settings.theme === 'dark' ? '0.3' : '0.4',
			'mix-blend-mode': settings.theme === 'dark' ? 'screen' : 'multiply'
		};
	}

	function drawMark(m: Mark) {
		try {
			rendition?.annotations.highlight(m.cfi, { id: m.id }, () => options.onMark?.(m.id), 'legejo-mark', markStyle());
		} catch {
			// A passage the chapter no longer has; nothing to draw.
		}
	}

	function eraseMark(m: Mark) {
		try {
			rendition?.annotations.remove(m.cfi, 'highlight');
		} catch {
			// Already gone with its page.
		}
	}

	/** All chapters on screen (two in a spread). */
	function allContents(): Contents[] {
		return (rendition?.getContents() as unknown as Contents[]) ?? [];
	}

	/** The chapter whose frame is under a point of the top window, and the point in it. */
	function contentsAt(x: number, y: number): { contents: Contents; x: number; y: number } | null {
		for (const contents of allContents()) {
			const frame = contents.document.defaultView?.frameElement;
			if (!frame) continue;
			const rect = frame.getBoundingClientRect();
			if (x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) continue;
			return { contents, x: x - rect.left, y: y - rect.top };
		}
		return null;
	}

	/** Where a point of a document falls in its text, in either browser dialect. */
	function caretAt(doc: Document, x: number, y: number): { node: Node; offset: number } | null {
		const d = doc as Document & {
			caretRangeFromPoint?: (x: number, y: number) => Range | null;
			caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null;
		};
		if (d.caretPositionFromPoint) {
			const at = d.caretPositionFromPoint(x, y);
			return at ? { node: at.offsetNode, offset: at.offset } : null;
		}
		const range = d.caretRangeFromPoint?.(x, y);
		return range ? { node: range.startContainer, offset: range.startOffset } : null;
	}

	// ---- Search highlight ---------------------------------------------------

	let highlighted: string | null = null;

	function mark(cfi: string) {
		const p = palettes[settings.theme];
		try {
			rendition?.annotations.highlight(cfi, {}, undefined, 'legejo-hit', {
				fill: p.link,
				'fill-opacity': '0.28',
				'mix-blend-mode': settings.theme === 'dark' ? 'screen' : 'multiply'
			});
		} catch {
			// The passage isn't on a rendered page; nothing to mark.
		}
	}

	function unmark(cfi: string) {
		try {
			rendition?.annotations.remove(cfi, 'highlight');
		} catch {
			// Already gone with its page.
		}
	}

	/**
	 * Wait for the shown chapter's fonts and images. They load after epub.js has
	 * paginated, and the reflow they cause moves text between pages. Resolves to
	 * whether anything was still loading.
	 */
	async function settled(r: InternalRendition): Promise<boolean> {
		const pending: Promise<unknown>[] = [];
		for (const contents of (r.getContents() as unknown as Contents[]) ?? []) {
			const doc = contents.document;
			if (!doc) continue;
			if (doc.fonts && doc.fonts.status !== 'loaded') pending.push(doc.fonts.ready);
			for (const img of doc.images) {
				if (!img.complete) pending.push(img.decode().catch(() => {}));
			}
		}
		if (pending.length === 0) return false;
		await Promise.race([
			Promise.all(pending),
			new Promise((resolve) => setTimeout(resolve, SETTLE_TIMEOUT_MS))
		]);
		// Let epub.js notice the new content size before positions are read again.
		await new Promise((resolve) => setTimeout(resolve, 60));
		return true;
	}

	/**
	 * epub.js reports a page's start CFI at the whitespace before its first
	 * word. Shown again, that CFI resolves to the end of the previous page, so a
	 * restored position would slip back one page per visit. Step forward when
	 * the target lies beyond what ended up on screen.
	 */
	async function correctPage(r: InternalRendition, target: string) {
		if (!paginated || !target.startsWith('epubcfi(')) return;
		const shown = r.currentLocation() as unknown as Location | undefined;
		if (!shown?.end?.cfi || shown.end.index !== spine.get(target)?.index) return;
		if (r.epubcfi.compare(target, shown.end.cfi) > 0) await r.next();
	}

	const watched = new WeakSet<HTMLElement>();

	/**
	 * Room below the last line of a chapter for the "next chapter" control.
	 * epub.js sizes a chapter to its text, so padding on the body would be cut off.
	 */
	function padScroller() {
		const el = scroller();
		if (!el) return;
		el.style.boxSizing = 'border-box';
		el.style.paddingBottom = SCROLL_END_PAD + 'px';
		if (watched.has(el)) return;
		watched.add(el);
		// epub.js deliberately skips the scroll event after one of its own jumps,
		// which can also swallow the reader's first scroll in a new chapter.
		let timer: ReturnType<typeof setTimeout> | null = null;
		el.addEventListener(
			'scroll',
			() => {
				if (timer) clearTimeout(timer);
				timer = setTimeout(() => {
					if (!destroyed && scroller() === el) rendition?.reportLocation();
				}, 150);
			},
			{ passive: true }
		);
	}

	async function place(r: InternalRendition, target: string) {
		await timeout(r.display(target), DISPLAY_TIMEOUT_MS);
		padScroller();
		await correctPage(r, target).catch(() => {});
	}

	async function show(target?: string | null) {
		const r = rendition;
		if (!r) return;
		try {
			if (target) await place(r, target);
			else {
				await timeout(r.display(), DISPLAY_TIMEOUT_MS);
				padScroller();
			}
		} catch (err) {
			if (!target || r !== rendition) throw err;
			// A stale or foreign CFI: start from the beginning instead of failing.
			await timeout(r.display(), DISPLAY_TIMEOUT_MS);
			return;
		}
		// The target was placed on a page before late resources reflowed the
		// text; place it again now that the layout is final.
		if (target && (await settled(r)) && r === rendition) {
			await place(r, target).catch(() => {});
		}
	}

	function enqueue(task: () => Promise<void>): Promise<void> {
		const run = queue.then(() => (destroyed ? undefined : task()));
		queue = run.catch(() => {});
		return run;
	}

	/** Relocations reported for a short while after a layout are not the reader's doing. */
	function endLayoutPass() {
		if (layoutTimer) clearTimeout(layoutTimer);
		layoutTimer = setTimeout(() => (layoutPass = false), 300);
	}

	let rerenderWaiting: Promise<void> | null = null;

	/**
	 * Rebuild the rendition at the current position (resize, flow, typography).
	 * A rebuild that hasn't started yet picks up the latest size and settings,
	 * so requests made in the meantime (a held-down A+) join it.
	 */
	function rerender(): Promise<void> {
		if (rerenderWaiting) return rerenderWaiting;
		const run = enqueue(async () => {
			rerenderWaiting = null;
			if (layoutTimer) clearTimeout(layoutTimer);
			layoutPass = true;
			build();
			try {
				await show(anchor);
			} finally {
				endLayoutPass();
			}
		});
		rerenderWaiting = run;
		run.catch(() => {
			if (rerenderWaiting === run) rerenderWaiting = null;
		});
		return run;
	}

	function restyle() {
		host.style.background = palettes[settings.theme].bg;
		css = contentCss(settings, cssContext) + offscreenCss;
		const contents = (rendition?.getContents() as unknown as Contents[]) ?? [];
		for (const c of contents) c.addStylesheetCss(css, STYLE_KEY);
		if (highlighted) {
			unmark(highlighted);
			mark(highlighted);
		}
		if (spokenMark) {
			const cfi = spokenMark;
			unmarkSpoken();
			markSpoken(cfi);
		}
	}

	let resizeTimer: ReturnType<typeof setTimeout> | null = null;
	const observer = new ResizeObserver(() => {
		if (resizeTimer) clearTimeout(resizeTimer);
		resizeTimer = setTimeout(() => {
			resizeTimer = null;
			const next = measure();
			if (destroyed || next.width === 0 || next.height === 0) return;
			if (next.width === size.width && next.height === size.height) return;
			rerender().catch(() => {});
		}, 150);
	});

	// ---- First display ----------------------------------------------------

	build();
	try {
		let target = options.initialCfi ?? null;
		if (!target && options.initialPercent && options.initialPercent > 0) {
			// Only a percentage is known (last read on another kind of device):
			// it can't be turned into a position until locations exist.
			await locationsDone;
			if (locationsReady) target = book.locations.cfiFromPercentage(clamp01(options.initialPercent));
		}
		await show(target);
	} catch {
		teardown();
		viewport.remove();
		book.destroy();
		throw new ReaderError('invalid');
	}
	observer.observe(host);
	endLayoutPass();

	locationsDone.then(() => {
		if (destroyed || !locationsReady) return;
		options.onLocationsReady?.();
		indexAnchors();
		// Re-report so the estimate is replaced by the exact percentage.
		rendition?.reportLocation();
	});

	// ---- Navigation -------------------------------------------------------

	function scrollScreen(direction: 1 | -1): boolean {
		const el = scroller();
		if (!el) return false;
		const atEdge =
			direction > 0
				? el.scrollTop + el.clientHeight >= el.scrollHeight - SCROLL_END_PAD - 4
				: el.scrollTop <= 4;
		if (atEdge) return false;
		el.scrollBy({
			top: direction * Math.max(80, el.clientHeight - 64),
			behavior: reducedMotion() ? 'auto' : 'smooth'
		});
		return true;
	}

	async function prevChapter(toEnd: boolean) {
		const r = rendition;
		if (!r) return;
		if (paginated) {
			const here = currentIndex();
			const target = sections.findLast((s) => s.index < here);
			if (target) await r.display(target.href);
			return;
		}
		await r.prev();
		const el = scroller();
		if (toEnd && el) el.scrollTop = el.scrollHeight;
	}

	/**
	 * Label of the TOC entry that covers a place in the book: a chapter file
	 * and, to tell entries within one file apart, the overall position (0–1).
	 */
	function labelAt(index: number, fraction: number | null): string | null {
		let label: string | null = null;
		for (const entry of flatToc) {
			if (entry.index < 0 || entry.index > index) continue;
			if (
				entry.index === index &&
				fraction !== null &&
				entry.start !== undefined &&
				entry.start > fraction
			) {
				continue;
			}
			label = entry.item.label;
		}
		return label;
	}

	function currentIndex(): number {
		return rendition?.location?.start?.index ?? 0;
	}

	// ---- Reading aloud --------------------------------------------------------

	/** The sentences of the chapter last asked for, each with where it is. */
	let spoken: { range: Range; text: string }[] = [];
	/** CFI of the sentence that is marked. */
	let spokenMark: string | null = null;

	function shownContents(): Contents | null {
		const all = (rendition?.getContents() as unknown as Contents[]) ?? [];
		const index = currentIndex();
		return all.find((c) => c.sectionIndex === index) ?? all[0] ?? null;
	}

	function collectSpoken(doc: Document, language: string) {
		const out: { range: Range; text: string }[] = [];
		const body = doc.body;
		// One paragraph at a time: its text, and the node each stretch came from.
		let nodes: { node: Text; start: number }[] = [];
		let text = '';
		const locate = (offset: number, end: boolean) => {
			let i = nodes.length - 1;
			while (i > 0 && nodes[i].start > offset) i--;
			// An end offset on a node boundary belongs to the node before it.
			if (end && i > 0 && nodes[i].start === offset) i--;
			return { node: nodes[i].node, offset: Math.min(nodes[i].node.length, offset - nodes[i].start) };
		};
		const flush = () => {
			if (nodes.length > 0 && /[\p{L}\p{N}]/u.test(text)) {
				for (const [from, to] of sentenceSpans(text, language)) {
					const sentence = text.slice(from, to);
					const said = sentence.replace(/\s+/g, ' ').trim();
					// Punctuation and ornaments on their own are not read.
					if (!/[\p{L}\p{N}]/u.test(said)) continue;
					const lead = sentence.length - sentence.trimStart().length;
					const trail = sentence.length - sentence.trimEnd().length;
					const a = locate(from + lead, false);
					const b = locate(to - trail, true);
					const range = doc.createRange();
					try {
						range.setStart(a.node, a.offset);
						range.setEnd(b.node, b.offset);
					} catch {
						continue;
					}
					out.push({ range, text: said });
				}
			}
			nodes = [];
			text = '';
		};
		let block: Node | null = null;
		const walker = doc.createTreeWalker(body, NodeFilter.SHOW_TEXT);
		for (let node = walker.nextNode(); node; node = walker.nextNode()) {
			const value = node.nodeValue ?? '';
			if (!value || unspoken(node, body)) continue;
			const parent = blockOf(node);
			if (parent !== block) flush();
			block = parent;
			nodes.push({ node: node as Text, start: text.length });
			text += value;
		}
		flush();
		return out;
	}

	/** The first and last position on screen, as ranges in the chapter. */
	function screenEdges(contents: Contents): { start: Range | null; end: Range | null } {
		const shown = rendition?.currentLocation() as unknown as Location | undefined;
		const at = (cfi?: string) => {
			try {
				return cfi ? contents.range(cfi) : null;
			} catch {
				return null;
			}
		};
		return { start: at(shown?.start?.cfi), end: at(shown?.end?.cfi) };
	}

	function onScreen(contents: Contents, range: Range): boolean {
		const { start, end } = screenEdges(contents);
		try {
			if (start && start.comparePoint(range.endContainer, range.endOffset) < 0) return false;
			if (end && end.comparePoint(range.startContainer, range.startOffset) > 0) return false;
		} catch {
			// Ranges from another document: the chapter has changed.
			return false;
		}
		return true;
	}

	function unmarkSpoken() {
		if (!spokenMark) return;
		try {
			rendition?.annotations.remove(spokenMark, 'highlight');
		} catch {
			// Already gone with its page.
		}
		spokenMark = null;
	}

	function markSpoken(cfi: string) {
		const p = palettes[settings.theme];
		try {
			rendition?.annotations.highlight(cfi, {}, undefined, 'legejo-spoken', {
				fill: p.link,
				'fill-opacity': '0.2',
				'mix-blend-mode': settings.theme === 'dark' ? 'screen' : 'multiply'
			});
			spokenMark = cfi;
		} catch {
			// Not on a rendered page; nothing to mark.
		}
	}

	const engine: ReaderEngine = {
		metadata,
		toc,
		get location() {
			return current;
		},
		get locationsReady() {
			return locationsReady;
		},

		next() {
			return enqueue(async () => {
				if (!rendition || scrollScreen(1)) return;
				await rendition.next();
			});
		},
		prev() {
			return enqueue(async () => {
				if (!rendition || scrollScreen(-1)) return;
				if (paginated) await rendition.prev();
				else await prevChapter(true);
			});
		},
		nextChapter() {
			return enqueue(async () => {
				if (!rendition) return;
				if (!paginated) return rendition.next();
				const target = sections.find((s) => s.index > currentIndex());
				if (target) await rendition.display(target.href);
			});
		},
		prevChapter() {
			return enqueue(() => prevChapter(false));
		},
		nudge(direction) {
			scroller()?.scrollBy({ top: direction * 56 });
		},
		goTo(target) {
			return enqueue(() => show(target));
		},
		goToPercent(fraction) {
			return enqueue(async () => {
				if (!locationsReady) return;
				await show(book.locations.cfiFromPercentage(clamp01(fraction)));
			});
		},
		chapterAt(fraction) {
			if (!locationsReady) return null;
			let index: number;
			try {
				index = spine.get(book.locations.cfiFromPercentage(clamp01(fraction)))?.index ?? -1;
			} catch {
				return null;
			}
			return labelAt(index, fraction);
		},
		async search(query, { signal, onHits, onProgress }) {
			const words = query.trim().split(/\s+/).filter(Boolean);
			if (words.length === 0) return false;
			// Case-insensitive, and any run of whitespace (a line break in the
			// source, say) matches a space in the query.
			const pattern = new RegExp(words.map(escapeRegExp).join('\\s+'), 'giu');
			let count = 0;

			for (let i = 0; i < sections.length; i++) {
				if (signal.aborted || destroyed) return false;
				const section = sections[i];
				const wasLoaded = !!section.contents;
				const hits: SearchHit[] = [];
				try {
					const root = await section.load(book.load.bind(book));
					const doc = root.ownerDocument;
					const body = root.querySelector('body') ?? root;

					// The chapter's text as one string, with the node each stretch
					// came from, so a match may span inline markup (<i>, <a> …).
					const nodes: { node: Text; start: number }[] = [];
					let text = '';
					let block: Node | null = null;
					const walker = doc.createTreeWalker(body, NodeFilter.SHOW_TEXT);
					for (let node = walker.nextNode(); node; node = walker.nextNode()) {
						const value = node.nodeValue ?? '';
						if (!value) continue;
						const parent = blockOf(node);
						// A new paragraph: keep words on either side of the break apart.
						if (block && parent !== block) text += '\n';
						block = parent;
						nodes.push({ node: node as Text, start: text.length });
						text += value;
					}

					const locate = (offset: number, end: boolean) => {
						let lo = 0;
						let hi = nodes.length - 1;
						while (lo < hi) {
							const mid = (lo + hi + 1) >> 1;
							if (nodes[mid].start <= offset) lo = mid;
							else hi = mid - 1;
						}
						// An end offset on a node boundary belongs to the node before it.
						if (end && lo > 0 && nodes[lo].start === offset) lo--;
						const { node, start } = nodes[lo];
						return { node, offset: Math.min(node.length, offset - start) };
					};
					// Whole words only at the cut ends of an excerpt.
					const squash = (part: string) => part.replace(/\s+/g, ' ');
					const lead = (part: string, cut: boolean) =>
						cut ? squash(part).replace(/^\S*\s/, '') : squash(part).trimStart();
					const trail = (part: string, cut: boolean) =>
						cut ? squash(part).replace(/\s\S*$/, '') : squash(part).trimEnd();

					pattern.lastIndex = 0;
					for (let m = pattern.exec(text); m && nodes.length > 0; m = pattern.exec(text)) {
						if (m[0].length === 0) break;
						const from = locate(m.index, false);
						const to = locate(m.index + m[0].length, true);
						const range = doc.createRange();
						range.setStart(from.node, from.offset);
						range.setEnd(to.node, to.offset);
						const rangeCfi = section.cfiFromRange(range);
						range.collapse(true);
						const cfi = section.cfiFromRange(range);
						const from0 = Math.max(0, m.index - SEARCH_CONTEXT_CHARS);
						const end = m.index + m[0].length;
						const to0 = Math.min(text.length, end + SEARCH_CONTEXT_CHARS);
						let at: number | null = null;
						if (locationsReady) {
							const percent = book.locations.percentageFromCfi(cfi);
							if (Number.isFinite(percent)) at = percent;
						}
						hits.push({
							cfi,
							rangeCfi,
							before: lead(text.slice(from0, m.index), from0 > 0),
							match: squash(m[0]),
							after: trail(text.slice(end, to0), to0 < text.length),
							chapterLabel: labelAt(section.index, at)
						});
						if (++count >= SEARCH_MAX_HITS) break;
					}
				} catch {
					// A chapter that can't be read is skipped.
				} finally {
					if (!wasLoaded) section.unload();
				}
				if (signal.aborted || destroyed) return false;
				if (hits.length > 0) onHits(hits);
				onProgress?.((i + 1) / sections.length);
				if (count >= SEARCH_MAX_HITS) return true;
				// Keep the page responsive: let input and rendering in between chapters.
				await new Promise((resolve) => setTimeout(resolve, 0));
			}
			return false;
		},
		highlight(cfi) {
			if (highlighted) unmark(highlighted);
			highlighted = cfi;
			if (cfi) mark(cfi);
		},
		selection(): Passage | null {
			for (const contents of allContents()) {
				const doc = contents.document;
				const sel = doc.getSelection();
				if (!sel || sel.isCollapsed || sel.rangeCount === 0 || !doc.body) continue;
				const range = sel.getRangeAt(0);
				if (!doc.body.contains(range.commonAncestorContainer)) continue;
				const text = range.toString().replace(/\s+/g, ' ').trim();
				if (!text) continue;
				try {
					return { cfi: contents.cfiFromRange(range), text };
				} catch {
					return null;
				}
			}
			return null;
		},
		clearSelection() {
			for (const contents of allContents()) contents.document.getSelection()?.removeAllRanges();
		},
		sentenceAt(x, y): Passage | null {
			const hit = contentsAt(x, y);
			if (!hit) return null;
			const doc = hit.contents.document;
			const caret = caretAt(doc, hit.x, hit.y);
			if (!caret || !doc.body?.contains(caret.node)) return null;
			const language = doc.documentElement.lang || metadata.language || '';
			for (const sentence of collectSpoken(doc, language)) {
				try {
					if (sentence.range.comparePoint(caret.node, caret.offset) !== 0) continue;
					return { cfi: hit.contents.cfiFromRange(sentence.range), text: sentence.text };
				} catch {
					// A point outside this sentence's nodes.
				}
			}
			return null;
		},
		passageBetween(from, to): Passage | null {
			const a = contentsAt(from.x, from.y);
			const b = contentsAt(to.x, to.y);
			if (!a || !b || a.contents !== b.contents) return null;
			const doc = a.contents.document;
			const start = caretAt(doc, a.x, a.y);
			const end = caretAt(doc, b.x, b.y);
			if (!start || !end || !doc.body?.contains(start.node) || !doc.body.contains(end.node)) return null;
			const range = doc.createRange();
			try {
				range.setStart(start.node, start.offset);
				range.setEnd(end.node, end.offset);
				// Dragged backwards: the same stretch, the other way round.
				if (range.collapsed) {
					range.setStart(end.node, end.offset);
					range.setEnd(start.node, start.offset);
				}
			} catch {
				return null;
			}
			const text = range.toString().replace(/\s+/g, ' ').trim();
			if (!text) return null;
			try {
				return { cfi: a.contents.cfiFromRange(range), text };
			} catch {
				return null;
			}
		},
		markAt(x, y): number | null {
			// The highlights are drawn as SVG in this document, over the chapter
			// frames, each with the id it was given; a point in one of its
			// rectangles is on it.
			for (const g of host.querySelectorAll<SVGGElement>('g.legejo-mark')) {
				const id = Number(g.dataset.id);
				if (!Number.isFinite(id)) continue;
				for (const rect of g.querySelectorAll('rect')) {
					const r = rect.getBoundingClientRect();
					if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return id;
				}
			}
			return null;
		},
		setMarks(next) {
			const keep = new Set(next.map((m) => m.id));
			for (const m of marks) if (!keep.has(m.id)) eraseMark(m);
			const shown = new Set(marks.map((m) => m.id));
			for (const m of next) if (!shown.has(m.id)) drawMark(m);
			marks = next.map((m) => ({ ...m }));
		},
		speechText(): SpeechText | null {
			const contents = shownContents();
			const doc = contents?.document;
			if (!contents || !doc?.body) return null;
			const language = doc.documentElement.lang || metadata.language || '';
			spoken = collectSpoken(doc, language);
			const { start } = screenEdges(contents);
			let first = 0;
			if (start) {
				// The first sentence that has not ended before the screen begins.
				first = spoken.findIndex((s) => {
					try {
						return s.range.comparePoint(start.startContainer, start.startOffset) <= 0;
					} catch {
						return true;
					}
				});
				if (first < 0) first = spoken.length;
			}
			return { sentences: spoken.map((s) => s.text), first, chapter: contents.sectionIndex, language };
		},
		speechShow(index) {
			let ok = true;
			return enqueue(async () => {
				unmarkSpoken();
				if (index === null) return;
				const sentence = spoken[index];
				const contents = shownContents();
				if (!sentence || !contents || !sentence.range.startContainer.isConnected || sentence.range.startContainer.ownerDocument !== contents.document) {
					ok = false;
					return;
				}
				if (!onScreen(contents, sentence.range)) {
					const at = sentence.range.cloneRange();
					at.collapse(true);
					await show(contents.cfiFromRange(at));
				}
				// Showing it may have laid the chapter out again.
				if (!sentence.range.startContainer.isConnected) {
					ok = false;
					return;
				}
				markSpoken((shownContents() ?? contents).cfiFromRange(sentence.range));
			}).then(() => ok);
		},
		speechOnScreen(index) {
			const sentence = spoken[index];
			const contents = shownContents();
			if (!sentence || !contents || !sentence.range.startContainer.isConnected) return false;
			return onScreen(contents, sentence.range);
		},
		async applySettings(next) {
			const previous = settings;
			settings = { ...next };
			const sameLayout =
				previous.fontSize === next.fontSize &&
				previous.fontFamily === next.fontFamily &&
				previous.lineHeight === next.lineHeight &&
				previous.margin === next.margin &&
				previous.flow === next.flow &&
				previous.hyphenate === next.hyphenate &&
				previous.spread === next.spread;
			if (sameLayout) {
				// Colours and alignment leave every line where it is.
				if (previous.theme !== next.theme || previous.textAlign !== next.textAlign) restyle();
				return;
			}
			// Anything that moves text changes pagination: lay the book out again.
			await rerender();
		},
		destroy() {
			if (destroyed) return;
			destroyed = true;
			observer.disconnect();
			if (resizeTimer) clearTimeout(resizeTimer);
			if (layoutTimer) clearTimeout(layoutTimer);
			teardown();
			viewport.remove();
			try {
				book.destroy();
			} catch {
				// Nothing left to release.
			}
		}
	};

	return engine;
}
