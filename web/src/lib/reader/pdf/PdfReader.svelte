<script lang="ts">
	// A reader for PDF books: the pages one after another, drawn by pdf.js as
	// they come into view, fitted to the width of the screen and zoomed with
	// the buttons, Ctrl and the wheel, or two fingers. Text is selectable in a
	// layer over each page (pdf.js's text layer), and a selection becomes a
	// highlight with the same bar as in the EPUB reader. A place and a passage
	// are kept as `pdf:` strings (anchor.ts) where an EPUB keeps its CFI.
	import { onMount, tick } from 'svelte';
	import { goto } from '$app/navigation';
	import { ArrowLeft, Highlighter, List, Minus, Moon, Plus, Search, Sun, SunMoon } from '@lucide/svelte';
	import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
	import type { PDFDocumentProxy, PDFPageProxy } from 'pdfjs-dist';
	import { t } from '#lib/i18n';
	import type { Annotation, BookDetail } from '#lib/types';
	import { savedVersion } from '#lib/offline/store';
	import AnnotateBar from '../AnnotateBar.svelte';
	import NotesPanel from '../NotesPanel.svelte';
	import Sheet from '../Sheet.svelte';
	import { createProgressStore } from '../progress';
	import { startReadingClock } from '../readingClock';
	import { loadSettings, saveSettings, THEMES } from '../settings';
	import { palettes } from '../themes';
	import type { ReaderTheme } from '../types';
	import '../reader.css';
	import { decodePassage, encodePage, encodePassage, mergeLines, pageOf, type PdfPassage, type PdfRect } from './anchor';

	type Props = { book: BookDetail; at: number | null; back: string | null };
	let { book, at, back }: Props = $props();

	// The route starts a new reader for another book (it is keyed on the book).
	// svelte-ignore state_referenced_locally
	const bookId = book.id;
	// svelte-ignore state_referenced_locally
	const backHref = back ?? `/books/${bookId}`;
	const fileUrl = `/api/books/${bookId}/file`;

	// ---- Look -------------------------------------------------------------------

	let settings = $state(loadSettings());
	const palette = $derived(palettes[settings.theme]);
	const THEME_ICONS = { light: Sun, sepia: SunMoon, dark: Moon } as const;
	const ThemeIcon = $derived(THEME_ICONS[settings.theme]);

	function nextTheme() {
		const i = THEMES.indexOf(settings.theme);
		settings = { ...settings, theme: THEMES[(i + 1) % THEMES.length] as ReaderTheme };
		saveSettings(settings);
	}

	// ---- The document -----------------------------------------------------------

	let status = $state<'loading' | 'ready' | 'error'>('loading');
	let failure = $state('');
	let downloaded = $state<number | null>(null);
	let pdf: PDFDocumentProxy | null = null;
	let total = $state(0);
	/** Each page's size at scale 1, in PDF points; the first page's until a page is seen. */
	let sizes = $state.raw<{ w: number; h: number }[]>([]);
	/** The outline: each entry's page, and how far down the page it points (a fraction; 0 when not said). */
	let outline = $state.raw<{ title: string; page: number; down: number; depth: number }[]>([]);

	// ---- Layout -----------------------------------------------------------------

	let scroller = $state<HTMLDivElement | null>(null);
	let width = $state(0);
	/** 1 fits the width of the screen (up to a readable width on a large one). */
	let zoom = $state(1);
	const ZOOM_MIN = 0.5;
	const ZOOM_MAX = 4;
	const GAP = 12;
	const MAX_FIT = 980;
	/** The width of a page at zoom 1, in CSS pixels. */
	const fit = $derived(Math.max(200, Math.min(width - 16, MAX_FIT)));
	const pageWidth = $derived(fit * zoom);
	/** Where each page starts and how tall it is, for finding the page at a scroll position. */
	/** The height of the bar at the top, which the first page starts below. */
	let headHeight = $state(56);
	const layout = $derived.by(() => {
		let y = headHeight + GAP;
		return sizes.map((s) => {
			const h = Math.round((pageWidth * s.h) / s.w);
			const box = { top: y, height: h };
			y += h + GAP;
			return box;
		});
	});
	const contentHeight = $derived(layout.length ? layout[layout.length - 1].top + layout[layout.length - 1].height + GAP : 0);

	// ---- Rendering --------------------------------------------------------------

	/** The page elements, by page number. */
	const pageEls = new Map<number, HTMLDivElement>();
	/** Pages drawn, and at which width; a page drawn at another width is drawn again. */
	const drawn = new Map<number, { width: number; task: { cancel(): void } | null }>();
	/** Pages near the screen, to draw. */
	let near = $state.raw<Set<number>>(new Set());
	const KEEP = 14;

	function pageNode(node: HTMLDivElement, n: number) {
		pageEls.set(n, node);
		observer?.observe(node);
		return {
			destroy() {
				observer?.unobserve(node);
				pageEls.delete(n);
			}
		};
	}

	let observer: IntersectionObserver | null = null;

	async function draw(n: number) {
		if (!pdf) return;
		const el = pageEls.get(n);
		if (!el) return;
		const want = Math.round(pageWidth);
		const done = drawn.get(n);
		if (done && done.width === want) return;
		done?.task?.cancel();
		const entry: { width: number; task: { cancel(): void } | null } = { width: want, task: null };
		drawn.set(n, entry);
		let page: PDFPageProxy;
		try {
			page = await pdf.getPage(n);
		} catch {
			return;
		}
		if (drawn.get(n) !== entry) return;
		const base = page.getViewport({ scale: 1 });
		// A page of another size than the first: its box follows it.
		if (Math.abs(base.width / base.height - sizes[n - 1].w / sizes[n - 1].h) > 0.01) {
			sizes = sizes.map((s, i) => (i === n - 1 ? { w: base.width, h: base.height } : s));
			await tick();
		}
		const scale = want / base.width;
		const viewport = page.getViewport({ scale });
		// Sharp on a dense screen, within what a phone will hold in a canvas.
		const ratio = Math.min(window.devicePixelRatio || 1, 2, Math.sqrt(12_000_000 / (viewport.width * viewport.height)));
		const canvas = document.createElement('canvas');
		canvas.width = Math.floor(viewport.width * ratio);
		canvas.height = Math.floor(viewport.height * ratio);
		const task = page.render({ canvas, viewport, transform: ratio !== 1 ? [ratio, 0, 0, ratio, 0, 0] : undefined });
		entry.task = task;
		try {
			await task.promise;
		} catch {
			return;
		}
		if (drawn.get(n) !== entry) return;
		entry.task = null;
		const slot = el.querySelector<HTMLDivElement>('.canvas');
		slot?.replaceChildren(canvas);
		// The text over the page, for selecting.
		const layer = el.querySelector<HTMLDivElement>('.textLayer');
		if (layer) {
			layer.replaceChildren();
			el.style.setProperty('--total-scale-factor', String(scale));
			const { TextLayer } = await pdfjs();
			const text = new TextLayer({ textContentSource: page.streamTextContent(), container: layer, viewport });
			await text.render().catch(() => {});
			const end = document.createElement('div');
			end.className = 'endOfContent';
			layer.append(end);
			endOfContent.set(layer, end);
		}
	}

	// The text of a page lies in pieces over it, with gaps between them. A
	// selection dragged into a gap would run on to the end of the page, so a
	// block that catches it is kept just after where the selection ends (or
	// before where it starts, when the start is the end being moved) while a
	// selection is being made, and goes back to the end of the page after.
	// The approach of pdf.js's own viewer (text_layer_builder.js).
	const endOfContent = new Map<HTMLElement, HTMLElement>();
	let lastRange: Range | null = null;

	function putBack(layer: HTMLElement, end: HTMLElement) {
		layer.append(end);
		end.style.width = '';
		end.style.height = '';
		end.style.userSelect = '';
		layer.classList.remove('selecting');
	}

	function putBackAll() {
		for (const [layer, end] of endOfContent) if (layer.isConnected) putBack(layer, end);
		lastRange = null;
	}

	function followSelection() {
		const selection = document.getSelection();
		if (!selection || selection.rangeCount === 0) {
			putBackAll();
			return;
		}
		const range = selection.getRangeAt(0);
		for (const [layer, end] of endOfContent) {
			if (!layer.isConnected) endOfContent.delete(layer);
			else if (range.intersectsNode(layer)) layer.classList.add('selecting');
			else putBack(layer, end);
		}
		// Which end is moving: the start, when the end stays where it was.
		const startMoves =
			!!lastRange &&
			(range.compareBoundaryPoints(Range.END_TO_END, lastRange) === 0 || range.compareBoundaryPoints(Range.START_TO_END, lastRange) === 0);
		let node: Node | null = startMoves ? range.startContainer : range.endContainer;
		if (node?.nodeType === Node.TEXT_NODE) node = node.parentNode;
		// A selection that ends at the very start of a piece ends in the one before it.
		if (!startMoves && range.endOffset === 0 && node) {
			do {
				while (node && !node.previousSibling) node = node.parentNode;
				node = node?.previousSibling ?? null;
			} while (node && !node.childNodes.length);
		}
		const piece = node as HTMLElement | null;
		const layer = piece?.parentElement?.closest<HTMLElement>('.textLayer');
		const end = layer ? endOfContent.get(layer) : undefined;
		if (piece && layer && end && piece !== end) {
			end.style.width = layer.style.width;
			end.style.height = layer.style.height;
			end.style.userSelect = 'text';
			piece.parentElement!.insertBefore(end, startMoves ? piece : piece.nextSibling);
		}
		lastRange = range.cloneRange();
	}

	/** Forget pages far from the screen, so a long book does not fill the memory. */
	function forgetFar() {
		if (drawn.size <= KEEP) return;
		const centre = current;
		const far = [...drawn.keys()].sort((a, b) => Math.abs(b - centre) - Math.abs(a - centre));
		for (const n of far.slice(0, drawn.size - KEEP)) {
			if (near.has(n)) continue;
			drawn.get(n)?.task?.cancel();
			drawn.delete(n);
			const el = pageEls.get(n);
			el?.querySelector('.canvas')?.replaceChildren();
			el?.querySelector('.textLayer')?.replaceChildren();
		}
	}

	$effect(() => {
		void pageWidth;
		void near;
		if (status !== 'ready') return;
		const timer = setTimeout(() => {
			for (const n of near) void draw(n);
			forgetFar();
		}, 80);
		return () => clearTimeout(timer);
	});

	let pdfjsModule: Promise<typeof import('pdfjs-dist')> | null = null;
	function pdfjs() {
		pdfjsModule ??= import('pdfjs-dist').then((m) => {
			m.GlobalWorkerOptions.workerSrc = workerUrl;
			return m;
		});
		return pdfjsModule;
	}

	// ---- Position ---------------------------------------------------------------

	const progress = createProgressStore(bookId);
	const clock = startReadingClock(bookId, () => (status === 'ready' ? 'reading' : 'away'));
	/** The page at the middle of the screen. */
	let current = $state(1);
	/** Saved only once the reader has moved: the page it opened at may come from another device. */
	let moved = false;
	let scrollFrame = 0;

	function pageAt(y: number): number {
		let lo = 0;
		let hi = layout.length - 1;
		while (lo < hi) {
			const mid = (lo + hi + 1) >> 1;
			if (layout[mid].top <= y) lo = mid;
			else hi = mid - 1;
		}
		return lo + 1;
	}

	function onScroll() {
		clock.touch();
		if (scrollFrame) return;
		scrollFrame = requestAnimationFrame(() => {
			scrollFrame = 0;
			if (!scroller || !layout.length) return;
			const middle = scroller.scrollTop + scroller.clientHeight / 2;
			const page = pageAt(middle);
			if (page !== current) {
				current = page;
				if (moved) savePosition();
			}
			placeBar();
		});
	}

	function savePosition() {
		if (!total) return;
		const box = layout[current - 1];
		const into = scroller && box ? Math.min(1, Math.max(0, (scroller.scrollTop + scroller.clientHeight / 2 - box.top) / box.height)) : 0;
		progress.save({ cfi: encodePage(current), percent: (current - 1 + into) / total });
	}

	/** Scroll a page, or a point on it (a fraction down the page), near the top of the screen. */
	function show(page: number, down = 0) {
		if (!scroller || !layout.length) return;
		const box = layout[Math.min(Math.max(page, 1), layout.length) - 1];
		scroller.scrollTo({ top: Math.max(0, box.top + box.height * down - 72) });
	}

	function go(page: number, down = 0) {
		moved = true;
		show(page, down);
		current = Math.min(Math.max(page, 1), total);
		savePosition();
	}

	// ---- Zoom -------------------------------------------------------------------

	/** Set the zoom, keeping the point at (x, y) of the screen where it is. */
	function setZoom(next: number, x = (scroller?.clientWidth ?? 0) / 2, y = (scroller?.clientHeight ?? 0) / 2) {
		if (!scroller) return;
		const z = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(next * 100) / 100));
		if (z === zoom) return;
		const ratio = z / zoom;
		const left = (scroller.scrollLeft + x) * ratio - x;
		const top = (scroller.scrollTop + y) * ratio - y;
		zoom = z;
		tick().then(() => {
			if (!scroller) return;
			scroller.scrollLeft = left;
			scroller.scrollTop = top;
		});
	}

	function onWheel(event: WheelEvent) {
		if (!event.ctrlKey && !event.metaKey) return;
		event.preventDefault();
		const box = scroller!.getBoundingClientRect();
		setZoom(zoom * Math.exp(-event.deltaY / 300), event.clientX - box.left, event.clientY - box.top);
	}

	// Two fingers: the pages scale as a picture while they move, and are drawn
	// anew at the new size when they are lifted.
	let pinch: { distance: number; x: number; y: number; zoom: number } | null = null;
	let pinchScale = $state(1);
	let pinchOrigin = $state('0 0');

	const spread = (e: TouchEvent) => Math.hypot(e.touches[0].clientX - e.touches[1].clientX, e.touches[0].clientY - e.touches[1].clientY);

	function onTouchStart(e: TouchEvent) {
		clock.touch();
		if (e.touches.length !== 2 || !scroller) return;
		const box = scroller.getBoundingClientRect();
		const x = (e.touches[0].clientX + e.touches[1].clientX) / 2 - box.left;
		const y = (e.touches[0].clientY + e.touches[1].clientY) / 2 - box.top;
		pinch = { distance: spread(e), x, y, zoom };
		pinchOrigin = `${scroller.scrollLeft + x}px ${scroller.scrollTop + y}px`;
	}

	function onTouchMove(e: TouchEvent) {
		if (!pinch || e.touches.length !== 2) return;
		e.preventDefault();
		const wanted = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, (pinch.zoom * spread(e)) / pinch.distance));
		pinchScale = wanted / pinch.zoom;
	}

	function onTouchEnd(e: TouchEvent) {
		if (!pinch || e.touches.length >= 2) return;
		const done = pinch;
		pinch = null;
		const scale = pinchScale;
		pinchScale = 1;
		if (Math.abs(scale - 1) > 0.02) setZoom(done.zoom * scale, done.x, done.y);
	}

	// ---- Bars -------------------------------------------------------------------

	let chrome = $state(true);
	let panel = $state<'toc' | 'search' | 'notes' | null>(null);

	function onTap(event: MouseEvent) {
		const selection = document.getSelection();
		if (selection && !selection.isCollapsed) return;
		const hit = markAt(event.clientX, event.clientY);
		if (hit) {
			openMark(hit);
			return;
		}
		if (pending || editing) {
			dismissAnnotate();
			return;
		}
		chrome = !chrome;
	}

	function onKey(event: KeyboardEvent) {
		const el = event.target as HTMLElement | null;
		if (el?.closest('input, textarea, select, dialog')) return;
		if (panel) return;
		clock.touch();
		if (event.key === 'ArrowRight' && !event.metaKey) {
			event.preventDefault();
			go(current + 1);
		} else if (event.key === 'ArrowLeft' && !event.metaKey) {
			event.preventDefault();
			go(current - 1);
		} else if (event.key === '+' || event.key === '=') {
			setZoom(zoom * 1.2);
		} else if (event.key === '-') {
			setZoom(zoom / 1.2);
		} else if (event.key === '0') {
			setZoom(1);
		} else if (event.key === 'Escape' && (pending || editing)) {
			dismissAnnotate();
		}
	}

	async function leave(event: MouseEvent) {
		if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
		event.preventDefault();
		await Promise.race([Promise.all([progress.flush(), clock.flush()]), new Promise((resolve) => setTimeout(resolve, 800))]);
		goto(backHref);
	}

	// ---- Contents and search ----------------------------------------------------

	async function readOutline(doc: PDFDocumentProxy) {
		const items = await doc.getOutline().catch(() => null);
		if (!items) return;
		const out: { title: string; page: number; down: number; depth: number }[] = [];
		const walk = async (list: typeof items, depth: number) => {
			for (const item of list) {
				let page: number | null = null;
				let down = 0;
				try {
					const dest = typeof item.dest === 'string' ? await doc.getDestination(item.dest) : item.dest;
					if (dest && dest[0]) {
						page = (await doc.getPageIndex(dest[0])) + 1;
						// [page, {name: 'XYZ'}, left, top, zoom]: top counts up from the foot of the page.
						const top = (dest as unknown[])[3];
						if ((dest[1] as { name?: string } | undefined)?.name === 'XYZ' && typeof top === 'number') {
							const height = (await doc.getPage(page)).view[3];
							if (height > 0) down = Math.min(1, Math.max(0, 1 - top / height));
						}
					}
				} catch {
					// An entry that points nowhere in the file.
				}
				if (page) out.push({ title: item.title, page, down, depth });
				if (item.items?.length) await walk(item.items, depth + 1);
			}
		};
		await walk(items, 0);
		outline = out;
	}

	/** The outline entry a point of a page is under, for a highlight's place. */
	function chapterOf(page: number, down: number): string | undefined {
		let found: { title: string; page: number; down: number } | undefined;
		for (const entry of outline) {
			const before = entry.page < page || (entry.page === page && entry.down <= down + 0.005);
			if (before && (!found || entry.page > found.page || (entry.page === found.page && entry.down >= found.down))) found = entry;
		}
		return found?.title;
	}

	let query = $state('');
	let searching = $state<number | null>(null);
	let hits = $state.raw<{ page: number; before: string; match: string; after: string }[]>([]);
	let searched = $state('');
	const HIT_LIMIT = 200;
	const fold = (s: string) => s.toLocaleLowerCase().normalize('NFD').replace(/\p{M}/gu, '');

	async function search(event: SubmitEvent) {
		event.preventDefault();
		const words = query.trim();
		if (!pdf || !words) return;
		const doc = pdf;
		const needle = fold(words);
		const found: typeof hits = [];
		searched = words;
		for (let n = 1; n <= total && found.length < HIT_LIMIT; n++) {
			searching = n;
			const content = await (await doc.getPage(n)).getTextContent();
			const text = content.items.map((i) => ('str' in i ? i.str : '')).join(' ').replace(/\s+/g, ' ');
			const folded = fold(text);
			for (let at = folded.indexOf(needle); at >= 0 && found.length < HIT_LIMIT; at = folded.indexOf(needle, at + needle.length)) {
				found.push({
					page: n,
					before: text.slice(Math.max(0, at - 40), at),
					match: text.slice(at, at + words.length),
					after: text.slice(at + words.length, at + words.length + 60)
				});
			}
			hits = [...found];
		}
		searching = null;
	}

	// ---- Highlights -------------------------------------------------------------

	let annotations = $state.raw<Annotation[]>([]);
	/** The passages of the highlights, by id. */
	const passages = $derived(new Map(annotations.flatMap((a) => (decodePassage(a.cfi) ? [[a.id, decodePassage(a.cfi)!] as const] : []))));
	let pending = $state.raw<{ passage: PdfPassage; text: string } | null>(null);
	let editing = $state.raw<Annotation | null>(null);
	let saving = $state(false);
	let annotateError = $state<string | null>(null);
	/** The bar goes where the passage is not. */
	let barAtTop = $state(false);

	async function loadAnnotations() {
		const res = await fetch(`/api/books/${bookId}/annotations`).catch(() => null);
		if (res?.ok) annotations = await res.json();
	}

	function marksOn(page: number): { id: number; rect: PdfRect }[] {
		const out: { id: number; rect: PdfRect }[] = [];
		for (const [id, passage] of passages) for (const part of passage) if (part.page === page) for (const rect of part.rects) out.push({ id, rect });
		return out;
	}

	/** The highlight under a point of the screen, if any; a little round each line counts too. */
	function markAt(x: number, y: number): Annotation | null {
		for (const [n, el] of pageEls) {
			const box = el.getBoundingClientRect();
			if (x < box.left || x > box.right || y < box.top || y > box.bottom) continue;
			const fx = (x - box.left) / box.width;
			const fy = (y - box.top) / box.height;
			for (const { id, rect } of marksOn(n)) {
				const slack = rect.height * 0.3;
				if (fx >= rect.left && fx <= rect.left + rect.width && fy >= rect.top - slack && fy <= rect.top + rect.height + slack) {
					return annotations.find((a) => a.id === id) ?? null;
				}
			}
		}
		return null;
	}

	/** What is selected in the pages, as a passage; null for nothing, or a selection elsewhere. */
	function selectedPassage(): { passage: PdfPassage; text: string } | null {
		const selection = document.getSelection();
		if (!selection || selection.isCollapsed || selection.rangeCount === 0 || !scroller) return null;
		const range = selection.getRangeAt(0);
		if (!scroller.contains(range.commonAncestorContainer)) return null;
		const text = selection.toString().replace(/\s+/g, ' ').trim();
		if (!text) return null;
		const byPage = new Map<number, PdfRect[]>();
		const boxes = [...pageEls].map(([n, el]) => [n, el.getBoundingClientRect()] as const);
		for (const r of range.getClientRects()) {
			if (r.width < 1 || r.height < 1) continue;
			const cx = r.left + r.width / 2;
			const cy = r.top + r.height / 2;
			const hit = boxes.find(([, b]) => cx >= b.left && cx <= b.right && cy >= b.top && cy <= b.bottom);
			if (!hit) continue;
			const [n, b] = hit;
			const rects = byPage.get(n) ?? [];
			rects.push({ top: (r.top - b.top) / b.height, left: (r.left - b.left) / b.width, height: r.height / b.height, width: r.width / b.width });
			byPage.set(n, rects);
		}
		const passage = [...byPage].sort((a, b) => a[0] - b[0]).map(([page, rects]) => ({ page, rects: mergeLines(rects) }));
		return passage.length ? { passage, text } : null;
	}

	let selectTimer: ReturnType<typeof setTimeout> | null = null;
	function onSelectionChange() {
		followSelection();
		if (selectTimer) clearTimeout(selectTimer);
		selectTimer = setTimeout(() => {
			const found = selectedPassage();
			// A selection going away does not close the bar: pressing one of
			// its buttons takes the selection with it, before the press lands.
			// A tap on the page, cancel or a save closes it.
			if (found) {
				editing = null;
				pending = found;
				annotateError = null;
			}
			placeBar();
		}, 250);
	}

	/** Put the bar on the side of the screen away from the passage. */
	function placeBar() {
		const passage = pending?.passage ?? (editing ? passages.get(editing.id) : null);
		if (!passage || !scroller) return;
		const first = passage[0];
		const el = pageEls.get(first.page);
		if (!el) return;
		const box = el.getBoundingClientRect();
		const top = box.top + box.height * first.rects[0].top;
		barAtTop = top > window.innerHeight / 2;
	}

	function openMark(a: Annotation) {
		document.getSelection()?.removeAllRanges();
		pending = null;
		editing = a;
		annotateError = null;
		placeBar();
	}

	function dismissAnnotate() {
		pending = null;
		editing = null;
		annotateError = null;
		document.getSelection()?.removeAllRanges();
	}

	async function saveHighlight(note: string | null) {
		if (!pending || saving) return;
		saving = true;
		annotateError = null;
		const { passage, text } = pending;
		const page = passage[0].page;
		try {
			const res = await fetch(`/api/books/${bookId}/annotations`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ cfi: encodePassage(passage), text, note, place: { chapter: chapterOf(page, passage[0].rects[0]?.top ?? 0), percent: (page - 1) / Math.max(1, total), page: String(page) } })
			});
			if (!res.ok) throw new Error(String(res.status));
			annotations = [...annotations, await res.json()];
			dismissAnnotate();
		} catch {
			annotateError = t('edit.saveFailed');
		} finally {
			saving = false;
		}
	}

	async function saveNote(note: string | null) {
		if (!editing || saving) return;
		saving = true;
		annotateError = null;
		try {
			const res = await fetch(`/api/annotations/${editing.id}`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ note })
			});
			if (!res.ok) throw new Error(String(res.status));
			const changed: Annotation = await res.json();
			annotations = annotations.map((a) => (a.id === changed.id ? changed : a));
			dismissAnnotate();
		} catch {
			annotateError = t('edit.saveFailed');
		} finally {
			saving = false;
		}
	}

	async function deleteAnnotation() {
		if (!editing || saving) return;
		const target = editing;
		saving = true;
		try {
			const res = await fetch(`/api/annotations/${target.id}`, { method: 'DELETE' });
			if (!res.ok && res.status !== 404) throw new Error(String(res.status));
			annotations = annotations.filter((a) => a.id !== target.id);
			dismissAnnotate();
		} catch {
			annotateError = t('edit.saveFailed');
		} finally {
			saving = false;
		}
	}

	function goToAnnotation(a: Annotation) {
		const passage = decodePassage(a.cfi);
		panel = null;
		if (passage) go(passage[0].page, passage[0].rects[0]?.top ?? 0);
	}

	// ---- Opening ----------------------------------------------------------------

	onMount(() => {
		const html = document.documentElement;
		html.classList.add('legejo-reading');
		const resize = new ResizeObserver(() => (width = scroller?.clientWidth ?? 0));
		if (scroller) resize.observe(scroller);
		width = scroller?.clientWidth ?? 0;
		observer = new IntersectionObserver(
			(entries) => {
				const next = new Set(near);
				for (const e of entries) {
					const n = Number((e.target as HTMLElement).dataset.page);
					if (e.isIntersecting) next.add(n);
					else next.delete(n);
				}
				near = next;
			},
			{ root: scroller, rootMargin: '150% 0px' }
		);
		for (const el of pageEls.values()) observer.observe(el);
		const flush = () => {
			progress.flush();
			clock.flush();
		};
		const onVisibility = () => {
			if (document.visibilityState === 'hidden') flush();
		};
		document.addEventListener('selectionchange', onSelectionChange);
		document.addEventListener('pointerup', putBackAll);
		document.addEventListener('visibilitychange', onVisibility);
		window.addEventListener('pagehide', flush);
		let closed = false;

		(async () => {
			try {
				const saved = await progress.load();
				const { getDocument } = await pdfjs();
				// A book saved on the device is answered whole by the service
				// worker, never in parts: it is read whole there too.
				const whole = (await savedVersion(bookId).catch(() => null)) !== null || !navigator.onLine;
				const task = getDocument({ url: fileUrl, disableRange: whole, disableAutoFetch: true });
				task.onProgress = ({ loaded, total: size }: { loaded: number; total: number }) => {
					if (size) downloaded = loaded / size;
				};
				const doc = await task.promise;
				if (closed) {
					void task.destroy();
					return;
				}
				pdf = doc;
				const first = (await doc.getPage(1)).getViewport({ scale: 1 });
				total = doc.numPages;
				sizes = Array.from({ length: total }, () => ({ w: first.width, h: first.height }));
				status = 'ready';
				void readOutline(doc);
				await loadAnnotations();
				await tick();
				// Where to open: a highlight asked for, else where the reader was.
				const mark = at ? annotations.find((a) => a.id === at) : null;
				const passage = mark ? decodePassage(mark.cfi) : null;
				if (passage) show(passage[0].page, passage[0].rects[0]?.top ?? 0);
				else if (saved) {
					const page = pageOf(saved.cfi) ?? Math.floor((saved.percent ?? 0) * total) + 1;
					show(page);
				}
			} catch (e) {
				status = 'error';
				const name = (e as { name?: string })?.name;
				failure = name === 'MissingPDFException' || name === 'ResponseException' ? t('reader.error.notFound') : name === 'InvalidPDFException' ? t('reader.pdf.invalid') : t('reader.error.network');
			}
		})();

		return () => {
			closed = true;
			flush();
			clock.stop();
			resize.disconnect();
			observer?.disconnect();
			document.removeEventListener('selectionchange', onSelectionChange);
			document.removeEventListener('pointerup', putBackAll);
			document.removeEventListener('visibilitychange', onVisibility);
			window.removeEventListener('pagehide', flush);
			for (const d of drawn.values()) d.task?.cancel();
			void pdf?.loadingTask.destroy();
			html.classList.remove('legejo-reading');
		};
	});

	// Reading on from where it opened: the position is the reader's own from now on.
	function onUserScroll() {
		moved = true;
	}
</script>

<svelte:window onkeydown={onKey} />

<div
	class="reader pdf"
	style:--r-bg={palette.bg}
	style:--r-fg={palette.fg}
	style:--r-muted={palette.muted}
	style:--r-link={palette.link}
	style:--r-surface={palette.surface}
	style:--r-border={palette.border}
	data-theme={settings.theme}
>
	<header class="bar top" class:hidden={!chrome} bind:clientHeight={headHeight}>
		<a class="r-icon" href={backHref} onclick={leave} aria-label={t('reader.back')} title={t('reader.back')}>
			<ArrowLeft size={20} />
		</a>
		<div class="heading">
			<h1>{book.title}</h1>
			{#if book.author}<p>{book.author}</p>{/if}
		</div>
		<button class="r-icon" onclick={() => (panel = 'search')} disabled={status !== 'ready'} aria-label={t('reader.search')} title={t('reader.search')}>
			<Search size={19} />
		</button>
		<button class="r-icon" onclick={() => (panel = 'toc')} disabled={status !== 'ready'} aria-label={t('reader.toc')} title={t('reader.toc')}>
			<List size={20} />
		</button>
		<button class="r-icon notes" onclick={() => (panel = 'notes')} disabled={status !== 'ready'} aria-label={t('reader.notes')} title={t('reader.notes')}>
			<Highlighter size={19} />
			{#if annotations.length > 0}<span class="count">{annotations.length}</span>{/if}
		</button>
		<button class="r-icon" onclick={nextTheme} aria-label={t('reader.theme')} title={t('reader.theme')}>
			<ThemeIcon size={19} />
		</button>
	</header>

	<!-- The pages. Selecting text and tapping highlights happen here; the bars
	     and panels are the accessible way to everything else. -->
	<div
		class="scroller"
		bind:this={scroller}
		onscroll={onScroll}
		onwheel={onWheel}
		ontouchstart={onTouchStart}
		ontouchmove={onTouchMove}
		ontouchend={onTouchEnd}
		ontouchcancel={onTouchEnd}
		onpointerdown={onUserScroll}
		onkeydown={onUserScroll}
		onclick={onTap}
		role="presentation"
	>
		{#if status === 'ready'}
			<div
				class="pages"
				style:height={`${contentHeight + 64}px`}
				style:width={`${Math.max(pageWidth + 16, width)}px`}
				style:transform={pinchScale !== 1 ? `scale(${pinchScale})` : null}
				style:transform-origin={pinchOrigin}
			>
				{#each layout as box, i (i)}
					{@const n = i + 1}
					<div
						class="page"
						data-page={n}
						use:pageNode={n}
						style:top={`${box.top}px`}
						style:width={`${Math.round(pageWidth)}px`}
						style:height={`${box.height}px`}
						style:--scale-round-x="1px"
						style:--scale-round-y="1px"
					>
						<div class="canvas"></div>
						<div class="marks" aria-hidden="true">
							{#each marksOn(n) as { id, rect }, j (id + '-' + j)}
								<span
									class="mark"
									class:open={editing?.id === id}
									style:top={`${rect.top * 100}%`}
									style:left={`${rect.left * 100}%`}
									style:height={`${rect.height * 100}%`}
									style:width={`${rect.width * 100}%`}
								></span>
							{/each}
							{#each pending?.passage.filter((part) => part.page === n) ?? [] as part, k (k)}
								{#each part.rects as rect, j (j)}
									<span
										class="mark held"
										style:top={`${rect.top * 100}%`}
										style:left={`${rect.left * 100}%`}
										style:height={`${rect.height * 100}%`}
										style:width={`${rect.width * 100}%`}
									></span>
								{/each}
							{/each}
						</div>
						<div class="textLayer"></div>
					</div>
				{/each}
			</div>
		{:else if status === 'loading'}
			<div class="notice" role="status">
				<p class="title">{book.title}</p>
				<div class="progressbar" class:indeterminate={downloaded === null}>
					<span style:width={`${(downloaded ?? 0) * 100}%`}></span>
				</div>
				<p>{t('reader.loading')}</p>
			</div>
		{:else}
			<div class="notice" role="alert">
				<p>{failure}</p>
				<a href={backHref}>{t('reader.back')}</a>
			</div>
		{/if}
	</div>

	{#if status === 'ready'}
		<footer class="bar bottom" class:hidden={!chrome}>
			<button class="r-icon" onclick={() => setZoom(zoom / 1.25)} disabled={zoom <= ZOOM_MIN} aria-label={t('reader.pdf.zoomOut')} title={t('reader.pdf.zoomOut')}>
				<Minus size={18} />
			</button>
			<button class="fit" onclick={() => setZoom(1)} disabled={zoom === 1} title={t('reader.pdf.fitWidth')}>
				{Math.round(zoom * 100)} %
			</button>
			<button class="r-icon" onclick={() => setZoom(zoom * 1.25)} disabled={zoom >= ZOOM_MAX} aria-label={t('reader.pdf.zoomIn')} title={t('reader.pdf.zoomIn')}>
				<Plus size={18} />
			</button>
			<input
				type="range"
				min="1"
				max={total}
				value={current}
				aria-label={t('reader.position')}
				onchange={(e) => go(Number(e.currentTarget.value))}
			/>
			<span class="where">{t('reader.pdf.pageOf', { page: current, total })}</span>
		</footer>
	{/if}

	{#if (pending || editing) && status === 'ready'}
		<AnnotateBar
			text={editing ? editing.text : (pending?.text ?? '')}
			note={editing ? editing.note : null}
			editing={editing !== null}
			{saving}
			error={annotateError}
			raised={chrome}
			top={barAtTop}
			onhighlight={saveHighlight}
			onsave={saveNote}
			ondelete={deleteAnnotation}
			oncancel={dismissAnnotate}
		/>
	{/if}

	<Sheet open={panel === 'toc'} title={t('reader.toc')} side="left" onclose={() => (panel = null)}>
		{#if outline.length === 0}
			<p class="empty">{t('reader.tocEmpty')}</p>
		{:else}
			<ol class="toc">
				{#each outline as entry, i (i)}
					<li style:padding-left={`${0.75 + entry.depth * 1}rem`}>
						<button
							onclick={() => {
								panel = null;
								go(entry.page);
							}}
						>
							<span>{entry.title}</span>
							<span class="page-no">{entry.page}</span>
						</button>
					</li>
				{/each}
			</ol>
		{/if}
	</Sheet>

	<Sheet open={panel === 'search'} title={t('reader.search')} side="right" onclose={() => (panel = null)}>
		<form class="search" onsubmit={search}>
			<!-- svelte-ignore a11y_autofocus -->
			<input type="search" bind:value={query} placeholder={t('reader.search.placeholder')} aria-label={t('reader.search.placeholder')} autofocus />
		</form>
		{#if searching !== null}
			<p class="empty">{t('reader.search.searching', { percent: Math.round((searching / Math.max(1, total)) * 100) })}</p>
		{/if}
		{#if searched && searching === null && hits.length === 0}
			<p class="empty">{t('reader.search.none', { query: searched })}</p>
		{:else if hits.length > 0}
			<p class="empty">
				{hits.length >= HIT_LIMIT ? t('reader.search.truncated', { count: HIT_LIMIT }) : hits.length === 1 ? t('reader.search.one') : t('reader.search.count', { count: hits.length })}
			</p>
			<ol class="hits">
				{#each hits as hit, i (i)}
					<li>
						<button
							onclick={() => {
								panel = null;
								go(hit.page);
							}}
						>
							<span class="page-no">{hit.page}</span>
							<span>…{hit.before}<mark>{hit.match}</mark>{hit.after}…</span>
						</button>
					</li>
				{/each}
			</ol>
		{/if}
	</Sheet>

	<NotesPanel
		open={panel === 'notes'}
		{annotations}
		exportHref={`/api/books/${bookId}/annotations/export`}
		onselect={goToAnnotation}
		onedit={(a) => {
			panel = null;
			const passage = decodePassage(a.cfi);
			if (passage) go(passage[0].page, passage[0].rects[0]?.top ?? 0);
			openMark(a);
		}}
		onclose={() => (panel = null)}
	/>
</div>

<style>
	.reader.pdf {
		position: fixed;
		inset: 0;
		display: flex;
		flex-direction: column;
		background: var(--r-bg);
		color: var(--r-fg);
	}
	.bar {
		position: absolute;
		left: 0;
		right: 0;
		z-index: 4;
		display: flex;
		align-items: center;
		gap: 0.1rem;
		background: color-mix(in srgb, var(--r-surface) 94%, transparent);
		-webkit-backdrop-filter: blur(12px);
		backdrop-filter: blur(12px);
		transition: transform 0.2s ease;
	}
	.bar.top {
		top: 0;
		padding: calc(0.2rem + env(safe-area-inset-top)) calc(0.35rem + env(safe-area-inset-right)) 0.2rem
			calc(0.35rem + env(safe-area-inset-left));
		border-bottom: 1px solid var(--r-border);
	}
	.bar.top.hidden {
		transform: translateY(-100%);
	}
	.bar.bottom {
		bottom: 0;
		gap: 0.25rem;
		padding: 0.25rem calc(0.6rem + env(safe-area-inset-right)) calc(0.25rem + env(safe-area-inset-bottom))
			calc(0.35rem + env(safe-area-inset-left));
		border-top: 1px solid var(--r-border);
	}
	.bar.bottom.hidden {
		transform: translateY(100%);
	}
	.heading {
		flex: 1;
		min-width: 0;
		padding: 0 0.4rem;
	}
	.heading h1,
	.heading p {
		margin: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.heading h1 {
		font-size: 0.95rem;
		line-height: 1.25;
	}
	.heading p {
		font-size: 0.78rem;
		color: var(--r-muted);
	}
	.notes {
		position: relative;
	}
	.count {
		position: absolute;
		top: 0.35rem;
		right: 0.3rem;
		font-size: 0.62rem;
		line-height: 1;
		padding: 0.12rem 0.25rem;
		border-radius: 99px;
		background: var(--r-link);
		color: var(--r-surface);
	}
	.fit {
		min-width: 3.4rem;
		font-size: 0.82rem;
		color: var(--r-fg);
		font-variant-numeric: tabular-nums;
	}
	.bar input[type='range'] {
		flex: 1;
		min-width: 4rem;
		accent-color: var(--r-link);
	}
	.where {
		font-size: 0.78rem;
		color: var(--r-muted);
		white-space: nowrap;
		font-variant-numeric: tabular-nums;
	}

	.scroller {
		position: relative;
		flex: 1;
		overflow: auto;
		overscroll-behavior: contain;
		touch-action: pan-x pan-y;
		-webkit-overflow-scrolling: touch;
	}
	.pages {
		position: relative;
		margin: 0 auto;
	}
	.page {
		position: absolute;
		left: 50%;
		translate: -50% 0;
		background: #fff;
		box-shadow: 0 1px 6px rgb(0 0 0 / 0.18);
		overflow: hidden;
	}
	.canvas,
	.canvas :global(canvas) {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
	}
	/* The themes reach the page: sepia warms it, dark turns it over (pictures
	   too). Only the picture of the page: the highlights keep their colour. */
	[data-theme='sepia'] .page {
		background: #f4ecd8;
	}
	[data-theme='sepia'] .canvas {
		filter: sepia(0.35) brightness(0.97);
	}
	[data-theme='dark'] .page {
		background: #1a1a1a;
	}
	[data-theme='dark'] .canvas {
		filter: invert(0.9) hue-rotate(180deg);
	}
	[data-theme='dark'] .mark {
		mix-blend-mode: screen;
		opacity: 0.3;
	}
	.marks {
		position: absolute;
		inset: 0;
		pointer-events: none;
		z-index: 1;
	}
	.mark {
		position: absolute;
		background: #f2c94c;
		opacity: 0.45;
		mix-blend-mode: multiply;
		border-radius: 2px;
	}
	.mark.open {
		opacity: 0.7;
	}
	/* What is about to be highlighted, still there once the selection is gone. */
	.mark.held {
		background: #7aa7e0;
	}

	/* pdf.js's text layer, adapted from its pdf_viewer.css (Apache-2.0): the
	   text in place over the page, invisible, there to be selected. */
	.page :global(.textLayer) {
		position: absolute;
		inset: 0;
		overflow: clip;
		line-height: 1;
		text-align: initial;
		-webkit-text-size-adjust: none;
		text-size-adjust: none;
		forced-color-adjust: none;
		transform-origin: 0 0;
		z-index: 2;
		--min-font-size: 1;
		--text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size));
		--min-font-size-inv: calc(1 / var(--min-font-size));
	}
	.page :global(.textLayer :is(span, br)) {
		color: transparent;
		position: absolute;
		white-space: pre;
		cursor: text;
		transform-origin: 0% 0%;
		-webkit-user-select: text;
		user-select: text;
	}
	.page :global(.textLayer > :not(.markedContent)),
	.page :global(.textLayer .markedContent span:not(.markedContent)) {
		z-index: 1;
		--font-height: 0;
		font-size: calc(var(--text-scale-factor) * var(--font-height));
		--scale-x: 1;
		--rotate: 0deg;
		transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv));
	}
	.page :global(.textLayer .markedContent) {
		display: contents;
	}
	.page :global(.textLayer ::selection) {
		background: color-mix(in srgb, #2f6fd6 35%, transparent);
		color: transparent;
	}
	.page :global(.textLayer br::selection) {
		background: transparent;
	}
	.page :global(.textLayer .endOfContent) {
		display: block;
		position: absolute;
		inset: 100% 0 0;
		z-index: 0;
		cursor: default;
		user-select: none;
	}
	.page :global(.textLayer.selecting .endOfContent) {
		top: 0;
	}

	.notice {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		padding: 30vh 1.5rem 0;
		text-align: center;
		color: var(--r-muted);
	}
	.notice .title {
		color: var(--r-fg);
		font-weight: 600;
	}
	.progressbar {
		width: min(16rem, 70vw);
		height: 3px;
		background: var(--r-border);
		border-radius: 2px;
		overflow: hidden;
	}
	.progressbar span {
		display: block;
		height: 100%;
		background: var(--r-link);
	}
	.progressbar.indeterminate span {
		width: 30% !important;
		animation: slide 1.1s ease-in-out infinite;
	}
	@keyframes slide {
		from {
			translate: -100% 0;
		}
		to {
			translate: 330% 0;
		}
	}

	.empty {
		margin: 0;
		padding: 0.75rem 1.25rem;
		color: var(--r-muted);
		font-size: 0.9rem;
	}
	.toc,
	.hits {
		list-style: none;
		margin: 0;
		padding: 0 0 1rem;
	}
	.toc button,
	.hits button {
		display: flex;
		gap: 0.75rem;
		width: 100%;
		text-align: left;
		padding: 0.55rem 1.25rem 0.55rem 0;
		font-size: 0.92rem;
		line-height: 1.35;
		color: var(--r-fg);
	}
	.hits button {
		padding-left: 1.25rem;
	}
	.toc button span:first-child {
		flex: 1;
	}
	.page-no {
		color: var(--r-muted);
		font-variant-numeric: tabular-nums;
		flex-shrink: 0;
	}
	.hits mark {
		background: color-mix(in srgb, var(--r-link) 25%, transparent);
		color: inherit;
		border-radius: 2px;
	}
	.search {
		padding: 0.5rem 1.25rem 0;
	}
	.search input {
		width: 100%;
		font: inherit;
		padding: 0.5rem 0.7rem;
		border-radius: 8px;
		border: 1px solid var(--r-border);
		background: var(--r-surface);
		color: var(--r-fg);
	}
	@media (prefers-reduced-motion: reduce) {
		.bar {
			transition: none;
		}
	}
</style>
