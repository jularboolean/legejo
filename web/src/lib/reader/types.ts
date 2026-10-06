// Shared types for the reader. Everything the Svelte components know about the
// rendering engine goes through these; none of them import epub.js.

export type ReaderTheme = 'light' | 'sepia' | 'dark';
export type ReaderFont = 'book' | 'serif' | 'sans';
export type ReaderFlow = 'paginated' | 'scrolled';
/** `book` leaves the alignment to the book's own stylesheet. */
export type ReaderAlign = 'book' | 'left' | 'justify';
/** Pages side by side on wide screens (`auto`), or always one page. */
export type ReaderSpread = 'auto' | 'single';

/** Per-device reading preferences (persisted in localStorage). */
export type ReaderSettings = {
	theme: ReaderTheme;
	/** Text size in percent of the book's own size. */
	fontSize: number;
	fontFamily: ReaderFont;
	/** Unitless line height. */
	lineHeight: number;
	/** Side margin in percent of the page width. */
	margin: number;
	flow: ReaderFlow;
	textAlign: ReaderAlign;
	/** Let the browser hyphenate, in the book's language. */
	hyphenate: boolean;
	spread: ReaderSpread;
	/** A short slide when the page turns (paginated flow). */
	animate: boolean;
};

/** Colours for one theme; used for the book content and the reader chrome. */
export type ThemePalette = {
	bg: string;
	fg: string;
	muted: string;
	link: string;
	/** Raised surfaces: toolbars and panels. */
	surface: string;
	border: string;
};

export type TocItem = {
	id: string;
	label: string;
	/** Target to pass to `ReaderEngine.goTo`. */
	href: string;
	depth: number;
	children: TocItem[];
};

export type ReaderLocation = {
	/** CFI of the first visible position; what gets saved as the reading position. */
	cfi: string;
	/** Overall progress, 0–1. Estimated from the spine until `exact` is true. */
	percent: number;
	/** True once the percentage comes from generated locations. */
	exact: boolean;
	/** Id of the TOC entry being read, if any. */
	tocId: string | null;
	chapterLabel: string | null;
	/** Page within the current chapter (paginated flow). */
	page: number;
	pagesInChapter: number;
	/** Pages after the ones on screen until the chapter ends (paginated flow). */
	pagesLeft: number;
	/** The end of the current chapter is on screen. */
	atChapterEnd: boolean;
	atStart: boolean;
	atEnd: boolean;
};

export type ReaderMetadata = {
	title: string;
	author: string;
	language: string;
	/** Right-to-left page progression. */
	rtl: boolean;
};

export type ReaderErrorCode = 'unauthorized' | 'not-found' | 'network' | 'invalid';

export class ReaderError extends Error {
	code: ReaderErrorCode;
	constructor(code: ReaderErrorCode, message?: string) {
		super(message ?? code);
		this.name = 'ReaderError';
		this.code = code;
	}
}

export type ReaderEngineOptions = {
	/** Element to render into. The engine fills it and follows its size. */
	host: HTMLElement;
	/** Same-origin URL of the EPUB file. */
	url: string;
	/** Identifies the book in the locations cache (one entry per book). */
	cacheKey: string;
	/** Changes when the file changes, which invalidates the cached locations. */
	cacheVersion: string;
	settings: ReaderSettings;
	/** CFI to open at, if any. */
	initialCfi?: string | null;
	/**
	 * Overall position (0–1) to open at when there is no CFI. Opening then waits
	 * for locations to be generated.
	 */
	initialPercent?: number | null;
	/** Download progress of the book file, 0–1; not called when the size is unknown. */
	onDownload?: (fraction: number) => void;
	onRelocated?: (location: ReaderLocation) => void;
	/** Locations are generated (or loaded from cache); percentages are now exact. */
	onLocationsReady?: () => void;
	/**
	 * Called for every chapter document that gets rendered. Book content lives in
	 * iframes, so input handlers have to be attached here, per document.
	 */
	onContent?: (doc: Document) => void;
};

export type SearchHit = {
	/** Target to pass to `ReaderEngine.goTo`. */
	cfi: string;
	/** The matched passage, to pass to `ReaderEngine.highlight`. */
	rangeCfi: string;
	/** The text around the match, split so the match itself can be marked. */
	before: string;
	match: string;
	after: string;
	chapterLabel: string | null;
};

export type SearchOptions = {
	/** Abort to stop a search that is no longer wanted. */
	signal: AbortSignal;
	/** Called with each batch of hits, in reading order. */
	onHits: (hits: SearchHit[]) => void;
	/** Share of the book searched so far, 0–1. */
	onProgress?: (fraction: number) => void;
};

/** The surface of the epub.js wrapper. Create with `openReader`. */
export interface ReaderEngine {
	readonly metadata: ReaderMetadata;
	readonly toc: TocItem[];
	/** Latest reported location, or null before the first page is shown. */
	readonly location: ReaderLocation | null;
	/** Whether jumping by percentage is available yet. */
	readonly locationsReady: boolean;

	/**
	 * One screen forward: the next page, or in scrolled flow one screenful,
	 * continuing into the next chapter at the end of the current one.
	 */
	next(): Promise<void>;
	prev(): Promise<void>;
	nextChapter(): Promise<void>;
	prevChapter(): Promise<void>;
	/** Scroll a few lines in scrolled flow (arrow keys); no-op when paginated. */
	nudge(direction: 1 | -1): void;
	/** Go to a TOC href or a CFI. */
	goTo(target: string): Promise<void>;
	/** Jump to an overall position, 0–1. No-op until `locationsReady`. */
	goToPercent(fraction: number): Promise<void>;
	/** Label of the chapter at an overall position; for previews while scrubbing. */
	chapterAt(fraction: number): string | null;
	/**
	 * Search the whole book, chapter by chapter. Resolves to whether the search
	 * was cut short because it reached the cap on the number of hits.
	 */
	search(query: string, options: SearchOptions): Promise<boolean>;
	/** Mark a passage (a search hit) on the page; null removes the mark. */
	highlight(cfi: string | null): void;
	/** Apply new settings, keeping the reading position. */
	applySettings(settings: ReaderSettings): Promise<void>;
	destroy(): void;
}
