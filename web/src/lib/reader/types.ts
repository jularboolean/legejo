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
	/** A highlight (see `ReaderEngine.setMarks`) was clicked or tapped. */
	onMark?: (id: number) => void;
};

/** A passage of the book: where it is, and what it says. */
export type Passage = {
	/** Range CFI, to pass to `ReaderEngine.goTo` and to store. */
	cfi: string;
	text: string;
};

/** A highlight to draw on the page. */
export type Mark = {
	id: number;
	cfi: string;
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

export type SpeechText = {
	sentences: string[];
	first: number;
	/** Which chapter this is, by its place in the book. */
	chapter: number;
	/** The chapter's language as the book states it; may be empty. */
	language: string;
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
	/** The text selected in the chapter on screen, if any. */
	selection(): Passage | null;
	clearSelection(): void;
	/**
	 * The sentence under a point of the top window, for where the chapter
	 * cannot be selected in (touch screens, WebKit). Null off the page.
	 */
	sentenceAt(x: number, y: number): Passage | null;
	/**
	 * The text between two points of the top window, in either order, for a
	 * drag where the chapter cannot be selected in. Null when the points are
	 * not both on one chapter, or nothing lies between them.
	 */
	passageBetween(from: { x: number; y: number }, to: { x: number; y: number }): Passage | null;
	/** The highlights to show; those already shown are kept, the rest redrawn. */
	setMarks(marks: Mark[]): void;
	/** The id of the highlight drawn under a point of the top window, if any. */
	markAt(x: number, y: number): number | null;
	/**
	 * The sentences of the chapter on screen, for reading aloud. `first` is
	 * the first one on screen (the count when none is left). Null when no
	 * chapter is shown.
	 */
	speechText(): SpeechText | null;
	/**
	 * Mark sentence `index` of the last `speechText()` and bring it on screen;
	 * null removes the mark. Resolves to false when the chapter has been laid
	 * out again since, and the text has to be fetched anew.
	 */
	speechShow(index: number | null): Promise<boolean>;
	/** Whether sentence `index` of the last `speechText()` is on screen. */
	speechOnScreen(index: number): boolean;
	/** Apply new settings, keeping the reading position. */
	applySettings(settings: ReaderSettings): Promise<void>;
	destroy(): void;
}
