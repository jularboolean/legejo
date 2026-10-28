<script lang="ts">
	import { page } from '$app/state';
	import './reader.css';
	import { onMount, tick } from 'svelte';
	import { goto, invalidateAll } from '$app/navigation';
	import { ChevronLeft, ChevronRight, ChevronsRight, TriangleAlert } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import type { Annotation, BookDetail } from '#lib/types';
	import { openReader } from './engine';
	import {
		attachGestures,
		followLinkAt,
		frameEventsWork,
		linkAt,
		type GestureHandlers
	} from './gestures';
	import { createProgressStore } from './progress';
	import { startReadingClock } from './readingClock';
	import { LIMITS, loadSettings, saveSettings } from './settings';
	import { palettes } from './themes';
	import {
		ReaderError,
		type ReaderEngine,
		type ReaderErrorCode,
		type ReaderLocation,
		type Passage,
		type PassageEdge,
		type ReaderSettings,
		type SearchHit,
		type TocItem
	} from './types';
	import { getLocale } from '#lib/i18n';
	import { languageLabel } from '#lib/library';
	import {
		createSpeech,
		loadSpeechPrefs,
		saveSpeechPrefs,
		speechSupported,
		voicesFor,
		type Speech,
		type SpeechPrefs,
		type SpeechState
	} from './speech';
	import AnnotateBar from './AnnotateBar.svelte';
	import ShareQuote from '#lib/ShareQuote.svelte';
	import HelpPanel from './HelpPanel.svelte';
	import NotesPanel from './NotesPanel.svelte';
	import SpeechBar from './SpeechBar.svelte';
	import ProgressBar from './ProgressBar.svelte';
	import ReaderToolbar from './ReaderToolbar.svelte';
	import SearchPanel from './SearchPanel.svelte';
	import SettingsPanel from './SettingsPanel.svelte';
	import TocPanel from './TocPanel.svelte';

	// One reader per book: the page keys this component on the book's id.
	let {
		book,
		at = null,
		back = null,
		find = null
	}: {
		book: BookDetail;
		/** A highlight to open the book at, by its id, instead of the reading position. */
		at?: number | null;
		/** Where the way back leads, when not to the book's page. */
		back?: string | null;
		/** Words to search the book for once it is open. */
		find?: string | null;
	} = $props();
	// svelte-ignore state_referenced_locally
	const bookId = book.id;
	// Without the network the book's own page is out of reach: back is to the books on the device.
	const backHref = $derived(page.data.offline ? '/offline' : (back ?? `/books/${bookId}`));

	type Panel = 'toc' | 'settings' | 'search' | 'help' | 'notes';

	/** How long the toolbars stay after the book opens, in ms. */
	const CHROME_INTRO_MS = 3500;
	/** …and after the mouse leaves them. */
	const CHROME_LEAVE_MS = 1200;
	/** Moving the mouse this close to the top or bottom edge brings the toolbars out, in px. */
	const EDGE_TOP_PX = 60;
	const EDGE_BOTTOM_PX = 90;
	/** How long the way back from a jump is offered, in ms. */
	const RETURN_MS = 20_000;
	const HINT_KEY = 'legejo.reader.hinted';

	let root: HTMLElement;
	/** The reading area; takes the reader's own pointer and key input. */
	let stage: HTMLElement;
	/** What the engine renders into. */
	let host: HTMLElement;

	let engine = $state.raw<ReaderEngine | null>(null);
	let status = $state<'loading' | 'ready' | 'error'>('loading');
	let errorCode = $state<ReaderErrorCode>('invalid');
	/** Share of the file downloaded, 0–1; null while unknown. */
	let downloaded = $state<number | null>(null);
	let settings = $state.raw<ReaderSettings>(loadSettings());
	let location = $state.raw<ReaderLocation | null>(null);
	let locationsReady = $state(false);
	let toc = $state.raw<TocItem[]>([]);

	let panel = $state<Panel | null>(null);
	/** null: no fullscreen API in this browser. */
	let fullscreen = $state<boolean | null>(null);
	/**
	 * Input can't be captured inside the chapter iframes (WebKit), so they are
	 * made transparent to the pointer and the stage takes the input instead.
	 */
	let shield = $state(false);
	/** Tap zones explained, the first time on a touch device. */
	let hint = $state(false);
	let toast = $state<string | null>(null);

	const paginated = $derived(settings.flow === 'paginated');
	const palette = $derived(palettes[settings.theme]);
	const rtl = $derived(engine?.metadata.rtl ?? false);

	// ---- Toolbars -------------------------------------------------------------

	/** Toolbars shown. They start visible and get out of the way by themselves. */
	let chrome = $state(true);
	/** The mouse is at the top or bottom edge, where the toolbars are. */
	let hovering = false;
	let chromeTimer: ReturnType<typeof setTimeout> | null = null;

	function showChrome() {
		if (chromeTimer) clearTimeout(chromeTimer);
		chromeTimer = null;
		chrome = true;
	}

	function hideChrome() {
		if (chromeTimer) clearTimeout(chromeTimer);
		chromeTimer = null;
		chrome = false;
	}

	/** Hide the toolbars after a while, unless they are in use by then. */
	function hideChromeLater(ms: number) {
		if (chromeTimer) clearTimeout(chromeTimer);
		chromeTimer = setTimeout(() => {
			chromeTimer = null;
			const focused = document.activeElement;
			const inUse =
				hovering || panel !== null || (focused instanceof HTMLElement && focused.closest('.toolbar, .progress'));
			if (!inUse) chrome = false;
		}, ms);
	}

	// ---- Reading position -------------------------------------------------------

	const progress = createProgressStore(bookId);
	/** Time spent reading, counted while the page is in view and the reader active. */
	const clock = startReadingClock(bookId, () =>
		status !== 'ready' ? 'away' : speechState === 'playing' ? 'listening' : 'reading'
	);
	// Positions are only saved once the reader has moved: the position a book
	// opens at may come from another device and must not be written back.
	let moved = false;
	let savedCfi: string | null = null;
	let savedExact = false;

	function onRelocated(next: ReaderLocation) {
		location = next;
		clock.touch();
		// Laid out again, perhaps: the handles go where the passage now is.
		placeEndsSoon();
		// The reader turned the page or jumped: reading aloud goes on from there.
		speech?.resync();
		if (!moved) return;
		// The same place is saved again once its percentage is exact: until the
		// positions are worked out it is an estimate, and the catalog shows it.
		if (next.cfi === savedCfi && (savedExact || !next.exact)) return;
		savedCfi = next.cfi;
		savedExact = next.exact;
		progress.save({ cfi: next.cfi, percent: next.percent, text: next.excerpt });
	}

	/**
	 * Leave for the book's page, once the position has reached the server:
	 * that page shows the progress, and would otherwise load the old one.
	 */
	async function leave(event: MouseEvent) {
		if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
		event.preventDefault();
		await Promise.race([Promise.all([progress.flush(), clock.flush()]), new Promise((resolve) => setTimeout(resolve, 800))]);
		goto(backHref);
	}

	// ---- Navigation -------------------------------------------------------------

	/** Where a jump (contents, search, slider, link) left from, to offer the way back. */
	let returnTo = $state<string | null>(null);
	let returnTimer: ReturnType<typeof setTimeout> | null = null;
	let turnsSinceJump = 0;

	function forgetReturn() {
		if (returnTimer) clearTimeout(returnTimer);
		returnTimer = null;
		returnTo = null;
	}

	/** Call before leaving the current page for somewhere else in the book. */
	function leaving() {
		moved = true;
		const from = location?.cfi;
		if (!from) return;
		returnTo = from;
		turnsSinceJump = 0;
		if (returnTimer) clearTimeout(returnTimer);
		returnTimer = setTimeout(forgetReturn, RETURN_MS);
	}

	function goBack() {
		const target = returnTo;
		forgetReturn();
		if (!target) return;
		moved = true;
		clearHighlight();
		engine?.goTo(target).catch(() => {});
		stage.focus({ preventScroll: true });
	}

	let toastTimer: ReturnType<typeof setTimeout> | null = null;
	function showToast(message: string) {
		toast = message;
		if (toastTimer) clearTimeout(toastTimer);
		toastTimer = setTimeout(() => (toast = null), 2200);
	}

	/** What a page turn leaves behind it, before the page moves. */
	function beforeTurn() {
		moved = true;
		hideChrome();
		clearHighlight();
		hoverNote = null;
		if (pending || editing) dismissAnnotate();
		// Reading on from where a jump landed: the way back is no longer wanted.
		if (returnTo && ++turnsSinceJump >= 3) forgetReturn();
	}

	function turn(forward: boolean) {
		if (!engine) return;
		if (forward && location?.atEnd) {
			showToast(t('reader.endOfBook'));
			return;
		}
		beforeTurn();
		const go = () => (forward ? engine!.next() : engine!.prev()).catch(() => {});
		if (!turnLeaf(forward, go)) go().then(() => slide(forward));
	}

	// ---- Reading aloud -----------------------------------------------------------

	const canSpeak = speechSupported();
	let speech: Speech | null = null;
	let speechState = $state<SpeechState>('off');
	let speechPrefs = $state.raw<SpeechPrefs>(loadSpeechPrefs());
	let speechLanguage = $state<string | null>(null);
	let speechVoices = $state.raw<SpeechSynthesisVoice[]>([]);
	/** The language a missing voice has been reported for; said once, not per chapter. */
	let voiceWarned: string | null = null;

	function toggleSpeech() {
		if (!engine || !canSpeak) return;
		speech ??= createSpeech({
			engine,
			prefs: () => speechPrefs,
			onState: (next) => {
				speechState = next;
				if (next === 'off') voiceWarned = null;
			},
			onLanguage: (language, hasVoice) => {
				speechLanguage = language;
				speechVoices = voicesFor(language);
				if (!hasVoice && language && voiceWarned !== language) {
					voiceWarned = language;
					showToast(t('reader.speech.noVoice', { language: languageLabel(language, getLocale()) }));
				}
			},
			onFinished: () => showToast(t('reader.endOfBook')),
			onError: () => showToast(t('reader.speech.failed'))
		});
		// Listening is reading: the position is saved as it moves on.
		moved = true;
		hideChrome();
		speech.toggle();
	}

	/** Browsers load their list of voices after the page; follow it. */
	function onVoices() {
		if (speechState !== 'off') speechVoices = voicesFor(speechLanguage);
	}

	function changeSpeechPrefs(next: SpeechPrefs) {
		speechPrefs = next;
		saveSpeechPrefs(next);
		speech?.refresh();
	}

	// ---- Page-turn animation ----------------------------------------------------

	/** Which way the page last came in from; the counter restarts the animation. */
	let slid = $state<{ forward: boolean; n: number } | null>(null);

	/** The page turn under way as a leaf, to cut short when the next one comes. */
	let leaf: ViewTransition | null = null;

	/**
	 * Turn the page as a sheet of paper, when the reader has asked for page
	 * animation and the browser can take a picture of the page as it was (a
	 * view transition: the chapters are in frames no other way can show).
	 * The picture is drawn aside, off the new page (reader.css). False when
	 * the page is to turn without it.
	 */
	function canTurnLeaf(): boolean {
		return (
			settings.animate &&
			paginated &&
			!!engine &&
			typeof document.startViewTransition === 'function' &&
			!matchMedia('(prefers-reduced-motion: reduce)').matches
		);
	}

	/** How long a page coming back waits outside for the book to lay it out. */
	const BACK_WAITS_MS = 1200;

	/** `held`: a finger moves the sheet, not the clock. */
	function turnLeaf(forward: boolean, go: () => Promise<unknown>, held = false): boolean {
		if (!canTurnLeaf() || !engine) return false;
		leaf?.skipTransition();
		const html = document.documentElement;
		const surface = engine.surface;
		// Seen from the reader: a next page in a right-to-left book turns the other way.
		const back = forward === rtl;
		html.dataset.turn = `${engine.spread ? 'spread' : 'single'}-${back ? 'back' : 'forward'}`;
		surface.style.setProperty('view-transition-name', 'legejo-leaf');
		// The browser draws nothing while it waits for the new page, and
		// epub.js waits for a frame to finish turning: the turn is set going,
		// and the transition goes on a moment later with the new page as it
		// comes (the new side of a view transition is live, not a picture).
		let turned: Promise<unknown> = Promise.resolve();
		const turning = document.startViewTransition(() =>
			Promise.race([(turned = go()), new Promise((resolve) => setTimeout(resolve, 60))])
		);
		leaf = turning;
		// The page coming back is the one that moves, and it is live: it
		// waits outside until the book has laid it out, then comes in whole.
		if (back && !held) {
			turning.ready
				.then(async () => {
					const animations = leafAnimations();
					for (const a of animations) a.pause();
					await Promise.race([turned, new Promise((resolve) => setTimeout(resolve, BACK_WAITS_MS))]);
					await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
					if (leaf === turning) for (const a of animations) a.play();
				})
				.catch(() => {});
		}
		turning.finished.finally(() => {
			if (leaf !== turning) return;
			leaf = null;
			delete html.dataset.turn;
			surface.style.removeProperty('view-transition-name');
		});
		return true;
	}

	// ---- A sheet that follows the finger ------------------------------------------

	/**
	 * A page pulled by a finger, on one page (not a spread): the sheet goes as
	 * far as the finger has gone across the page, and on letting go goes the
	 * rest of the way, or falls back and the page stays. The transition runs
	 * as a page turn does, held still and set by the finger.
	 */
	let pulled: {
		forward: boolean;
		width: number;
		progress: number;
		animations: Animation[] | null;
		transition: ViewTransition;
		turned: Promise<unknown>;
	} | null = null;

	/** A pull let go after this much of the page, or this fast, turns it. */
	const PULL_TURNS = 0.3;
	const PULL_FLING = 0.35;

	function leafAnimations(): Animation[] {
		return document
			.getAnimations()
			.filter((a) => String((a.effect as KeyframeEffect | null)?.pseudoElement ?? '').includes('legejo-leaf'));
	}

	function setPull(progress: number) {
		if (!pulled) return;
		pulled.progress = Math.max(0, Math.min(1, progress));
		for (const a of pulled.animations ?? []) {
			const duration = Number(a.effect?.getTiming().duration) || 0;
			a.currentTime = pulled.progress * duration;
		}
	}

	function onPull(p: { phase: 'start' | 'move' | 'end'; dx: number; vx: number }): boolean {
		if (p.phase === 'start') {
			if (pulled || !engine || engine.spread || !canTurnLeaf()) return false;
			// Seen from the reader: pulling to the left turns forward, in a right-to-left book back.
			const forward = (p.dx < 0) !== rtl;
			if (forward ? location?.atEnd : location?.atStart) return false;
			beforeTurn();
			let turned: Promise<unknown> = Promise.resolve();
			const go = () => (turned = (forward ? engine!.next() : engine!.prev()).catch(() => {}));
			if (!turnLeaf(forward, go, true) || !leaf) return false;
			const transition = leaf;
			pulled = { forward, width: engine.surface.getBoundingClientRect().width || window.innerWidth, progress: 0, animations: null, transition, turned };
			const mine = pulled;
			transition.ready
				.then(() => {
					if (pulled !== mine) return;
					mine.turned = turned;
					mine.animations = leafAnimations();
					for (const a of mine.animations) a.pause();
					setPull(mine.progress);
				})
				.catch(() => {
					if (pulled === mine) pulled = null;
				});
			setPull(Math.abs(p.dx) / pulled.width);
			return true;
		}
		const current = pulled;
		if (!current) return false;
		// How far the finger has gone the way the page turns.
		const toward = ((current.forward !== rtl ? -p.dx : p.dx) || 0) / current.width;
		if (p.phase === 'move') {
			setPull(toward);
			return true;
		}
		pulled = null;
		const speed = (current.forward !== rtl ? -p.vx : p.vx) || 0;
		if (current.progress >= PULL_TURNS || speed > PULL_FLING) {
			// The rest of the way, at the speed a turn has.
			for (const a of current.animations ?? []) a.play();
			if (!current.animations) current.transition.ready.then(() => leafAnimations().forEach((a) => a.play())).catch(() => {});
			return true;
		}
		void fallBack(current);
		return true;
	}

	/** The sheet falls back in place, and the page turns back under it to where it was. */
	async function fallBack(current: NonNullable<typeof pulled>) {
		const animations = current.animations ?? [];
		const start = performance.now();
		const from = current.progress;
		const ms = 180 * Math.max(0.3, from) + 60;
		await new Promise<void>((resolve) => {
			const step = (now: number) => {
				const left = Math.max(0, 1 - (now - start) / ms);
				for (const a of animations) a.currentTime = from * left * (Number(a.effect?.getTiming().duration) || 0);
				if (left > 0) requestAnimationFrame(step);
				else resolve();
			};
			requestAnimationFrame(step);
		});
		await current.turned;
		await (current.forward ? engine?.prev() : engine?.next())?.catch(() => {});
		current.transition.skipTransition();
	}

	/** Let the new page glide in, when the reader has asked for it. */
	function slide(forward: boolean) {
		if (!settings.animate || !paginated) return;
		// The page comes from where it lay: the far side for the next one.
		slid = { forward: forward !== rtl, n: (slid?.n ?? 0) + 1 };
	}

	async function goTo(item: TocItem) {
		leaving();
		clearHighlight();
		engine?.goTo(item.href).catch(() => {});
		await closePanel();
	}

	function seek(fraction: number) {
		leaving();
		clearHighlight();
		engine?.goToPercent(fraction).catch(() => {});
	}

	function nextChapter() {
		moved = true;
		engine?.nextChapter().catch(() => {});
	}

	// ---- Highlights and notes -----------------------------------------------------

	let annotations = $state.raw<Annotation[]>([]);
	/** A passage waiting for an action: selected, or the sentence under a long press. */
	let pending = $state<Passage | null>(null);
	/** A highlight opened for its note, or for removal. */
	let editing = $state<Annotation | null>(null);
	/** A highlight being made into a picture to post. */
	let sharing = $state<Annotation | null>(null);
	let annotating = $state(false);
	let annotateError = $state<string | null>(null);
	/** With a mouse, the note of the highlight under the pointer is shown beside it. */
	let hoverNote = $state<Annotation | null>(null);
	let hoverAt = $state({ x: 0, y: 0 });
	const noteTipAbove = $derived(hoverAt.y > window.innerHeight - 200);
	const noteTipLeft = $derived(Math.max(8, Math.min(hoverAt.x + 14, window.innerWidth - 360)));
	const exportHref = `/api/books/${bookId}/annotations/export`;

	async function loadAnnotations() {
		try {
			const res = await fetch(`/api/books/${bookId}/annotations`);
			if (res.ok) annotations = await res.json();
		} catch {
			// The highlights wait for the next visit.
			return;
		}
		placeFromDevice();
	}

	/**
	 * A highlight from a Kobo comes with the text and the chapter it is in,
	 * not with a place in the EPUB. The first time the book is opened here,
	 * each is looked up by its text and given its place, once and for all.
	 */
	async function placeFromDevice() {
		const unplaced = annotations.filter((a) => a.source === 'kobo' && !a.cfi).slice(0, 50);
		for (const a of unplaced) {
			if (closed || !engine) return;
			let chapter: string | null = null;
			try {
				chapter = JSON.parse(a.location ?? 'null')?.span?.chapterFilename ?? null;
			} catch {
				// A location in a shape not understood: search the whole book.
			}
			const cfi = await engine.locate(a.text, chapter).catch(() => null);
			if (!cfi) continue;
			try {
				const res = await fetch(`/api/annotations/${a.id}`, {
					method: 'PUT',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ cfi })
				});
				if (!res.ok) continue;
				const placed: Annotation = await res.json();
				annotations = annotations.map((x) => (x.id === placed.id ? placed : x));
			} catch {
				// Next time.
			}
		}
	}

	/** A passage's place, or nothing when the engine cannot tell. */
	async function placeOf(cfi: string) {
		try {
			return (await engine?.placeOf(cfi)) ?? null;
		} catch {
			return null;
		}
	}

	// Highlights made before places were kept, or before the book's
	// positions were worked out, get theirs once both are there. Written
	// apart from the highlight, so a device does not see it as changed.
	let placing = false;
	$effect(() => {
		if (!engine || !locationsReady || placing) return;
		const waiting = annotations.filter((a) => a.cfi && a.place?.percent == null);
		if (waiting.length === 0) return;
		placing = true;
		(async () => {
			for (const a of waiting) {
				const place = await placeOf(a.cfi as string);
				if (!place) continue;
				try {
					const res = await fetch(`/api/annotations/${a.id}`, {
						method: 'PUT',
						headers: { 'Content-Type': 'application/json' },
						body: JSON.stringify({ place })
					});
					if (res.ok) {
						const saved: Annotation = await res.json();
						annotations = annotations.map((x) => (x.id === saved.id ? saved : x));
					}
				} catch {
					// Next time the book is opened.
				}
			}
		})().finally(() => (placing = false));
	});

	// What the page shows follows the list.
	$effect(() => {
		engine?.setMarks(annotations.filter((a) => a.cfi).map((a) => ({ id: a.id, cfi: a.cfi as string })));
	});

	function offerSelection() {
		if (!engine || panel) return;
		const passage = engine.selection();
		if (passage) {
			editing = null;
			pending = passage;
		} else if (pending && !editing) {
			pending = null;
		}
	}

	/**
	 * A passage picked without a selection to show for it: mark it meanwhile.
	 * The bars of the reader are left as they are: brought out, they would
	 * cover the lines at the top and the foot of the page.
	 */
	function offer(passage: Passage) {
		editing = null;
		pending = passage;
		clearHighlight();
		engine?.highlight(passage.cfi);
	}

	/** A long press picks the word under it; off a word, the sentence. */
	function offerWord(point: { x: number; y: number }) {
		if (!engine || panel) return;
		const passage = engine.wordAt(point.x, point.y) ?? engine.sentenceAt(point.x, point.y);
		if (passage) offer(passage);
	}

	/** The whole sentences the passage being picked reaches into, when that is more. */
	const sentences = $derived.by(() => {
		if (!engine || !pending || editing || handleDrag) return null;
		const whole = engine.sentencesOf(pending.cfi);
		return whole && whole.cfi !== pending.cfi ? whole : null;
	});

	function widenToSentences() {
		if (!sentences) return;
		// Where the book can be selected in, the selection is widened and taken up from there.
		if (shield) offer(sentences);
		else engine?.select(sentences.cfi);
	}

	// ---- Handles: the ends of a passage being picked, to drag ---------------------

	/** Where the ends of the passage being picked stand; with the shield up only. */
	let ends = $state.raw<{ start: PassageEdge; end: PassageEdge } | null>(null);
	/**
	 * The end being dragged: which one, the passage as it was when it was
	 * taken, and how far the finger is from the point of the text it holds.
	 */
	let handleDrag = $state.raw<{ edge: 'start' | 'end'; cfi: string; dx: number; dy: number } | null>(null);
	let endsFrame = 0;

	function placeEnds() {
		ends = shield && engine && pending && !editing ? engine.edges(pending.cfi) : null;
	}

	/** Once a frame at most, for scrolling and resizing. */
	function placeEndsSoon() {
		if (endsFrame) return;
		endsFrame = requestAnimationFrame(() => {
			endsFrame = 0;
			placeEnds();
		});
	}

	$effect(() => {
		void [pending, editing, shield, engine];
		placeEnds();
	});

	/**
	 * The bar for the passage goes to the side of the page with more room,
	 * so that it does not cover the passage; it stays put while an end is
	 * dragged.
	 */
	let annotateAtTop = $state(false);
	$effect(() => {
		const cfi = editing?.cfi ?? pending?.cfi;
		void ends;
		if (!engine || !cfi || handleDrag) return;
		const at = engine.edges(cfi);
		if (at) annotateAtTop = at.start.top > window.innerHeight - at.end.bottom;
	});

	function grabEnd(event: PointerEvent, edge: 'start' | 'end') {
		if (!pending || !ends || event.button !== 0) return;
		event.preventDefault();
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		const at = ends[edge];
		// The point this end holds: just inside the passage, halfway up the line.
		const x = edge === 'start' ? at.x + 1 : at.x - 1;
		const y = (at.top + at.bottom) / 2;
		handleDrag = { edge, cfi: pending.cfi, dx: x - event.clientX, dy: y - event.clientY };
	}

	function dragEnd(event: PointerEvent) {
		const held = handleDrag;
		if (!held || !engine) return;
		const passage = engine.adjust(held.cfi, held.edge, { x: event.clientX + held.dx, y: event.clientY + held.dy });
		if (!passage || passage.cfi === pending?.cfi) return;
		pending = passage;
		engine.highlight(passage.cfi);
	}

	function dropEnd() {
		handleDrag = null;
	}

	/**
	 * With the shield up the browser draws no selection while the mouse is
	 * dragged: the passage it would be is marked as the drag goes, so that
	 * one sees what is being picked.
	 */
	function previewDrag(from: { x: number; y: number }, to: { x: number; y: number }) {
		if (!engine || panel || !shield) return;
		const passage = engine.passageBetween(from, to);
		if (passage) engine.highlight(passage.cfi);
	}

	/** With the shield up a drag cannot select; the text between its ends is the passage. */
	function offerDrag(from: { x: number; y: number }, to: { x: number; y: number }) {
		if (!engine || panel || !shield) return;
		const passage = engine.passageBetween(from, to);
		if (passage) offer(passage);
	}

	/** When a highlight was last opened: the same click must not turn the page. */
	let markOpenedAt = 0;

	function openMark(id: number) {
		const found = annotations.find((a) => a.id === id);
		if (!found) return;
		markOpenedAt = Date.now();
		pending = null;
		editing = found;
		annotateError = null;
		showChrome();
	}

	function dismissAnnotate() {
		if (pending && searchCurrent === null) engine?.highlight(null);
		pending = null;
		editing = null;
		annotateError = null;
		engine?.clearSelection();
	}

	async function saveHighlight(note: string | null) {
		if (!pending || annotating) return;
		annotating = true;
		annotateError = null;
		try {
			const res = await fetch(`/api/books/${bookId}/annotations`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ cfi: pending.cfi, text: pending.text, note, place: await placeOf(pending.cfi) })
			});
			if (!res.ok) throw new Error(String(res.status));
			const made: Annotation = await res.json();
			annotations = [...annotations, made];
			dismissAnnotate();
			showToast(t('reader.notes.saved'));
		} catch {
			annotateError = t('edit.saveFailed');
		} finally {
			annotating = false;
		}
	}

	async function saveNote(note: string | null) {
		if (!editing || annotating) return;
		annotating = true;
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
			annotating = false;
		}
	}

	async function deleteAnnotation() {
		if (!editing || annotating) return;
		const target = editing;
		annotating = true;
		try {
			const res = await fetch(`/api/annotations/${target.id}`, { method: 'DELETE' });
			if (!res.ok && res.status !== 404) throw new Error(String(res.status));
			annotations = annotations.filter((a) => a.id !== target.id);
			dismissAnnotate();
		} catch {
			annotateError = t('edit.saveFailed');
		} finally {
			annotating = false;
		}
	}

	function shareAnnotation() {
		const target = editing;
		dismissAnnotate();
		sharing = target;
	}

	async function goToAnnotation(a: Annotation) {
		if (!a.cfi) return;
		leaving();
		clearHighlight();
		engine?.goTo(a.cfi).catch(() => {});
		await closePanel();
	}

	async function editAnnotation(a: Annotation) {
		await closePanel();
		openMark(a.id);
	}

	// ---- Input ------------------------------------------------------------------

	let wheelAt = 0;
	const gestures: GestureHandlers = {
		area: () => host.getBoundingClientRect(),
		onTap(zone, point) {
			if (hint) return dismissHint();
			// A tap on a highlight opens it, whatever else lies under the point.
			const markId = engine?.markAt(point.x, point.y) ?? null;
			if (markId !== null) {
				openMark(markId);
				return;
			}
			if (shield && linkAt(host, point.x, point.y)) {
				leaving();
				if (!followLinkAt(host, point.x, point.y)) forgetReturn();
				return;
			}
			if (!paginated || zone === 'center') {
				if (chrome) hideChrome();
				else showChrome();
				return;
			}
			// epub.js answers a click on a highlight in a handler of its own,
			// which may run after this one: the page turns only once that has
			// had its turn and opened nothing.
			setTimeout(() => {
				if (Date.now() - markOpenedAt < 400) return;
				turn((zone === 'end') !== rtl);
			}, 0);
		},
		onPull,
		onSwipe(forward) {
			if (hint) dismissHint();
			if (paginated) turn(forward !== rtl);
		},
		onKey: handleKey,
		onLink: leaving,
		onPointer: pointerAt,
		onSelect: offerSelection,
		onLongPress: offerWord,
		onDragSelect: offerDrag,
		onDragMove: previewDrag,
		onWheel(deltaY) {
			// One page per wheel gesture; trackpads keep sending events while coasting.
			if (!paginated || Math.abs(deltaY) < 20 || Date.now() - wheelAt < 450) return;
			wheelAt = Date.now();
			turn(deltaY > 0);
		},
		onActivity() {
			clock.touch();
			if (paginated) return;
			moved = true;
			if (chrome && !hovering) hideChrome();
		}
	};

	/** Which page-turn zone the mouse is over, to hint at what a click there does. */
	let edge = $state<'start' | 'end' | null>(null);
	let pointerFrame = 0;

	/** A mouse that hovers; touch screens report strays of this kind after a tap. */
	const canHover = () => matchMedia('(hover: hover)').matches;

	function pointerAt(point: { x: number; y: number }) {
		if (!canHover()) return;
		const atEdge = point.y < EDGE_TOP_PX || point.y > window.innerHeight - EDGE_BOTTOM_PX;
		if (atEdge) {
			hovering = true;
			if (status === 'ready') showChrome();
		} else if (hovering) {
			hovering = false;
			hideChromeLater(CHROME_LEAVE_MS);
		}

		if (pointerFrame) return;
		pointerFrame = requestAnimationFrame(() => {
			pointerFrame = 0;
			if (status !== 'ready' || !host) return;
			const rect = host.getBoundingClientRect();
			const ratio = rect.width > 0 ? (point.x - rect.left) / rect.width : 0.5;
			const inPage = point.y > rect.top && point.y < rect.bottom;
			edge = !paginated || atEdge || !inPage ? null : ratio < 1 / 3 ? 'start' : ratio > 2 / 3 ? 'end' : null;
			// A highlight under the pointer: its note beside the pointer, and a hand.
			const markId = inPage ? (engine?.markAt(point.x, point.y) ?? null) : null;
			const mark = markId === null ? null : (annotations.find((a) => a.id === markId) ?? null);
			hoverNote = mark?.note ? mark : null;
			if (hoverNote) hoverAt = point;
			// With the shield up the browser no longer shows what is a link.
			if (shield) stage.style.cursor = inPage && (mark || linkAt(host, point.x, point.y)) ? 'pointer' : '';
		});
	}

	function onWindowPointerMove(event: PointerEvent) {
		if (event.pointerType === 'mouse') pointerAt({ x: event.clientX, y: event.clientY });
	}

	/** Keys that open and close things; they work whether or not a panel is open. */
	function handlePanelKey(event: KeyboardEvent): boolean {
		if (event.metaKey || event.ctrlKey || event.altKey) return false;
		switch (event.key) {
			case 't':
			case 'T':
				togglePanel('toc');
				return true;
			case 's':
			case 'S':
				togglePanel('settings');
				return true;
			case 'n':
			case 'N':
				togglePanel('notes');
				return true;
			case '/':
				togglePanel('search');
				return true;
			case 'l':
			case 'L':
				if (!canSpeak || status !== 'ready') return false;
				toggleSpeech();
				return true;
			case '?':
				togglePanel('help');
				return true;
			case 'f':
			case 'F':
				if (fullscreen === null) return false;
				toggleFullscreen();
				return true;
			case '+':
			case '=':
				stepFontSize(1);
				return true;
			case '-':
			case '−':
				stepFontSize(-1);
				return true;
		}
		return false;
	}

	function handleKey(event: KeyboardEvent): boolean {
		if (event.metaKey || event.ctrlKey || event.altKey) return false;
		if (hint) dismissHint();
		if (handlePanelKey(event)) return true;
		if (!engine || panel) return false;
		switch (event.key) {
			case 'ArrowRight':
				turn(!rtl);
				return true;
			case 'ArrowLeft':
				turn(rtl);
				return true;
			case 'PageDown':
				turn(true);
				return true;
			case 'PageUp':
				turn(false);
				return true;
			case ' ':
				turn(!event.shiftKey);
				return true;
			case 'ArrowDown':
			case 'ArrowUp': {
				const direction = event.key === 'ArrowDown' ? 1 : -1;
				if (paginated) turn(direction > 0);
				else {
					moved = true;
					engine.nudge(direction);
				}
				return true;
			}
			case 'Escape':
				if (chrome) hideChrome();
				else showChrome();
				return true;
			case 'Tab':
				// Tabbing from the page brings the (inert while hidden) toolbars back.
				showChrome();
				return false;
		}
		return false;
	}

	function onWindowKey(event: KeyboardEvent) {
		// The share dialog has the keys to itself.
		if (event.defaultPrevented || sharing) return;
		const el = event.target as Element | null;
		// Leave keys alone while a control is being operated.
		if (el?.closest?.('input, textarea, select, [contenteditable="true"]')) return;
		if ((event.key === ' ' || event.key === 'Enter') && el?.closest?.('button, a')) return;
		if (el?.closest?.('dialog')) {
			// Inside a panel only the keys that switch panels apply; Escape is the dialog's own.
			if (handlePanelKey(event)) event.preventDefault();
			return;
		}
		if (handleKey(event)) event.preventDefault();
	}

	/** Svelte action: reader input handling on one of this document's elements. */
	function surface(node: HTMLElement) {
		const detach = attachGestures(node, gestures);
		return { destroy: detach };
	}

	// ---- Panels, settings, fullscreen ---------------------------------------------

	function openPanel(next: Panel) {
		if ((next === 'toc' || next === 'search' || next === 'notes') && !engine) return;
		if (next === 'notes') dismissAnnotate();
		panel = next;
	}

	async function closePanel() {
		if (panel === null) return;
		panel = null;
		// Closing a dialog hands focus back to its toolbar button; reading goes
		// on from the page, where Space turns pages instead of reopening the panel.
		await tick();
		stage?.focus({ preventScroll: true });
		if (!hovering) hideChromeLater(CHROME_LEAVE_MS);
	}

	function togglePanel(next: Panel) {
		if (panel === next) closePanel();
		else openPanel(next);
	}

	function changeSettings(next: ReaderSettings) {
		settings = next;
		saveSettings(next);
		engine?.applySettings(next).catch(() => {});
	}

	function stepFontSize(direction: 1 | -1) {
		const { min, max, step } = LIMITS.fontSize;
		const fontSize = Math.min(max, Math.max(min, settings.fontSize + direction * step));
		if (fontSize !== settings.fontSize) changeSettings({ ...settings, fontSize });
	}

	function toggleFullscreen() {
		if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
		else root.requestFullscreen().catch(() => {});
	}

	function dismissHint() {
		hint = false;
		try {
			localStorage.setItem(HINT_KEY, '1');
		} catch {
			// It will show again next time; no harm done.
		}
	}

	// ---- Search -----------------------------------------------------------------

	let searchQuery = $state('');
	let searchHits = $state.raw<SearchHit[]>([]);
	let searching = $state(false);
	let searched = $state(0);
	let searchTruncated = $state(false);
	let searchCurrent = $state<string | null>(null);
	let searchAbort: AbortController | null = null;

	async function search(query: string) {
		searchAbort?.abort();
		searchAbort = null;
		clearHighlight();
		searchQuery = query;
		searchHits = [];
		searched = 0;
		searchTruncated = false;
		searching = false;
		if (!engine || !query) return;

		const abort = new AbortController();
		searchAbort = abort;
		searching = true;
		try {
			const truncated = await engine.search(query, {
				signal: abort.signal,
				onHits: (hits) => (searchHits = [...searchHits, ...hits]),
				onProgress: (fraction) => (searched = fraction)
			});
			if (!abort.signal.aborted) searchTruncated = truncated;
		} catch {
			// The results so far stand.
		} finally {
			if (searchAbort === abort) {
				searchAbort = null;
				searching = false;
			}
		}
	}

	async function goToHit(hit: SearchHit) {
		leaving();
		searchCurrent = hit.rangeCfi;
		const opened = engine;
		await closePanel();
		hideChrome();
		try {
			await opened?.goTo(hit.cfi);
			// Marked once the page is up, so the mark lands where the text is.
			if (searchCurrent === hit.rangeCfi) opened?.highlight(hit.rangeCfi);
		} catch {
			// The page didn't come up; nothing to mark.
		}
	}

	function clearHighlight() {
		if (searchCurrent === null) return;
		searchCurrent = null;
		engine?.highlight(null);
	}

	// ---- Opening ------------------------------------------------------------------

	let closed = false;

	/** How long a highlight visited from the list of notes stays marked. */
	const VISIT_MARK_MS = 4000;

	/** Where a highlight of this book is, or null. */
	async function highlightAt(id: number): Promise<string | null> {
		try {
			const res = await fetch(`/api/books/${bookId}/annotations`);
			if (!res.ok) return null;
			const list: Annotation[] = await res.json();
			return list.find((a) => a.id === id)?.cfi ?? null;
		} catch {
			return null;
		}
	}

	async function open() {
		status = 'loading';
		downloaded = null;
		try {
			const saved = await progress.load();
			if (closed) return;
			savedCfi = saved?.cfi ?? null;
			// Opened at a highlight (from the list of notes): there, not at the
			// reading position, which is left as it is until the reader moves on.
			const visit = at == null ? null : await highlightAt(at);
			if (closed) return;
			const opened = await openReader({
				host,
				url: `/api/books/${bookId}/file`,
				cacheKey: String(bookId),
				cacheVersion: `${book.file_size}:${book.updated_at ?? book.created_at}`,
				settings,
				initialCfi: visit ?? saved?.cfi,
				initialPercent: saved?.percent,
				onDownload: (fraction) => (downloaded = fraction),
				onRelocated,
				onLocationsReady: () => (locationsReady = true),
				onContent(doc) {
					if (shield) return;
					if (forceShield || !frameEventsWork(doc)) shield = true;
					else attachGestures(doc, gestures);
				},
				onMark: openMark
			});
			if (closed) {
				opened.destroy();
				return;
			}
			engine = opened;
			loadAnnotations();
			if (visit) {
				opened.highlight(visit);
				setTimeout(() => engine === opened && opened.highlight(null), VISIT_MARK_MS);
			}
			toc = opened.toc;
			location = opened.location;
			locationsReady = opened.locationsReady;
			status = 'ready';
			// Settings may have been changed while the book was loading.
			opened.applySettings(settings).catch(() => {});
			// Last read on a Kobo: opened at its percentage, then moved to the
			// line the device was on, found by its words. Not saved back: the
			// reader has not moved yet.
			if (!visit && !saved?.cfi && saved?.koboText) {
				const line = await opened.locate(saved.koboText, saved.koboChapter ?? null).catch(() => null);
				if (line && !closed && engine === opened && !moved) await opened.goTo(line).catch(() => {});
			}
			if (find) {
				openPanel('search');
				search(find);
				return;
			}
			hideChromeLater(CHROME_INTRO_MS);
			await tick();
			stage.focus({ preventScroll: true });
		} catch (err) {
			if (closed) return;
			const code = err instanceof ReaderError ? err.code : 'invalid';
			if (code === 'unauthorized') {
				// The session is gone: same route as the rest of the app.
				await invalidateAll();
				goto('/login');
				return;
			}
			errorCode = code;
			status = 'error';
		}
	}

	// `?shield` forces the WebKit input path, for testing it in other browsers.
	let forceShield = false;

	onMount(() => {
		const params = new URLSearchParams(window.location.search);
		forceShield = params.has('shield') || params.has('overlay');
		fullscreen = document.fullscreenEnabled ? !!document.fullscreenElement : null;
		try {
			hint = matchMedia('(pointer: coarse)').matches && !localStorage.getItem(HINT_KEY);
		} catch {
			hint = false;
		}

		// The reader fills the screen, edge to edge: the page behind it must not
		// scroll or bounce, and on phones it extends under the notch and the
		// home indicator (the layout keeps clear of them with safe-area insets).
		const html = document.documentElement;
		html.classList.add('legejo-reading');
		const viewport = document.querySelector<HTMLMetaElement>('meta[name="viewport"]');
		const viewportBefore = viewport?.content;
		if (viewport && !/viewport-fit/.test(viewport.content)) viewport.content += ', viewport-fit=cover';

		const onFullscreen = () => (fullscreen = !!document.fullscreenElement);
		const flush = () => {
			progress.flush();
			clock.flush();
		};
		// Anything the reader does here keeps the reading time counting. Input
		// inside a chapter's own document reaches the reader as page turns,
		// scrolling and the like (gestures.ts), which touch it too.
		const onInput = () => clock.touch();
		const onVisibility = () => {
			if (document.visibilityState === 'hidden') flush();
		};
		// Dragging the scrollbar in scrolled flow is reading, too.
		const onPointerDown = () => {
			if (!paginated) moved = true;
		};
		// If focus ends up inside a chapter whose document can't run listeners,
		// the keyboard would go dead; bring it back out.
		const onFocusIn = (event: FocusEvent) => {
			if (shield && event.target instanceof HTMLIFrameElement) stage.focus({ preventScroll: true });
		};
		document.addEventListener('fullscreenchange', onFullscreen);
		document.addEventListener('visibilitychange', onVisibility);
		if (canSpeak) speechSynthesis.addEventListener('voiceschanged', onVoices);
		window.addEventListener('pagehide', flush);
		for (const type of ['pointerdown', 'keydown', 'wheel', 'touchstart'] as const) window.addEventListener(type, onInput, { capture: true, passive: true });
		host.addEventListener('pointerdown', onPointerDown);
		stage.addEventListener('focusin', onFocusIn);

		open();

		return () => {
			closed = true;
			document.removeEventListener('fullscreenchange', onFullscreen);
			document.removeEventListener('visibilitychange', onVisibility);
			window.removeEventListener('pagehide', flush);
			for (const type of ['pointerdown', 'keydown', 'wheel', 'touchstart'] as const) window.removeEventListener(type, onInput, { capture: true });
			host?.removeEventListener('pointerdown', onPointerDown);
			stage?.removeEventListener('focusin', onFocusIn);
			for (const timer of [chromeTimer, returnTimer, toastTimer]) if (timer) clearTimeout(timer);
			if (pointerFrame) cancelAnimationFrame(pointerFrame);
			searchAbort?.abort();
			speech?.stop();
			if (canSpeak) speechSynthesis.removeEventListener('voiceschanged', onVoices);
			flush();
			clock.stop();
			engine?.destroy();
			html.classList.remove('legejo-reading');
			html.style.removeProperty('--reader-bg');
			if (viewport && viewportBefore !== undefined) viewport.content = viewportBefore;
		};
	});

	// The page behind the reader shows at the screen's edges on some phones.
	$effect(() => {
		document.documentElement.style.setProperty('--reader-bg', palette.bg);
	});

	const errorText = $derived(
		errorCode === 'not-found'
			? t('reader.error.notFound')
			: errorCode === 'network'
				? t('reader.error.network')
				: t('reader.error.invalid')
	);
	const coverUrl = $derived(
		book.has_cover ? `/api/books/${bookId}/cover?v=${encodeURIComponent(book.updated_at ?? book.created_at)}` : null
	);
	const chromeVisible = $derived(chrome || status !== 'ready');
</script>

<svelte:head>
	<meta name="theme-color" content={palette.bg} />
</svelte:head>

<svelte:window onkeydown={onWindowKey} onpointermove={onWindowPointerMove} onresize={placeEndsSoon} />

<div
	class="reader"
	class:dark={settings.theme === 'dark'}
	bind:this={root}
	style:--r-bg={palette.bg}
	style:--r-fg={palette.fg}
	style:--r-muted={palette.muted}
	style:--r-link={palette.link}
	style:--r-surface={palette.surface}
	style:--r-border={palette.border}
>
	<ReaderToolbar
		title={book.title}
		author={book.author}
		{backHref}
		onback={leave}
		visible={chromeVisible}
		ready={status === 'ready'}
		{fullscreen}
		onsearch={() => openPanel('search')}
		ontoc={() => openPanel('toc')}
		onnotes={() => openPanel('notes')}
		notes={annotations.length}
		onsettings={() => openPanel('settings')}
		onfullscreen={toggleFullscreen}
		onhelp={() => openPanel('help')}
		speaking={canSpeak ? speechState !== 'off' : null}
		onspeech={() => (speechState === 'off' ? toggleSpeech() : speech?.stop())}
	/>

	<!-- The book. Keyboard and pointer input is handled in `gestures`; the
	     buttons in the toolbars are the accessible equivalents. -->
	<div
		class="stage"
		class:busy={status !== 'ready'}
		class:scrolled={!paginated}
		class:shield
		bind:this={stage}
		use:surface
		onscrollcapture={placeEndsSoon}
		tabindex="-1"
	>
		<div
			class="page"
			class:slide-a={slid !== null && slid.n % 2 === 1}
			class:slide-b={slid !== null && slid.n % 2 === 0}
			class:back={slid !== null && !slid.forward}
			bind:this={host}
		></div>
	</div>

	{#if status === 'ready' && paginated}
		<div class="edge start" class:on={edge === 'start'} aria-hidden="true">
			<ChevronLeft size={28} />
		</div>
		<div class="edge end" class:on={edge === 'end'} aria-hidden="true">
			<ChevronRight size={28} />
		</div>
	{/if}

	{#if status === 'ready' && !paginated && location?.atChapterEnd && !location.atEnd}
		<button class="pill next-chapter" onclick={nextChapter}>
			{t('reader.nextChapter')}
			<ChevronsRight size={16} />
		</button>
	{/if}

	{#if toast}
		<div class="pill toast" class:lifted={speechState !== 'off'} role="status">{toast}</div>
	{/if}

	{#if hoverNote?.note && !pending && !editing && panel === null}
		<div class="note-tip" class:above={noteTipAbove} style:left={`${noteTipLeft}px`} style:top={`${hoverAt.y + 18}px`} role="tooltip">
			{hoverNote.note}
		</div>
	{/if}

	{#if ends && speechState === 'off'}
		{#each ['start', 'end'] as const as edge (edge)}
			{@const at = ends[edge]}
			<div
				class="handle {edge}"
				style:left={`${at.x}px`}
				style:top={`${at.top}px`}
				style:height={`${at.bottom - at.top}px`}
				role="presentation"
				onpointerdown={(event) => grabEnd(event, edge)}
				onpointermove={dragEnd}
				onpointerup={dropEnd}
				onpointercancel={dropEnd}
			></div>
		{/each}
	{/if}

	{#if (pending || editing) && speechState === 'off'}
		<AnnotateBar
			text={editing ? editing.text : (pending?.text ?? '')}
			note={editing ? editing.note : null}
			editing={editing !== null}
			saving={annotating}
			error={annotateError}
			raised={chromeVisible}
			top={annotateAtTop}
			faded={handleDrag !== null}
			lookupLanguage={page.data.dictionary && !page.data.offline ? (speechLanguage ?? book.language ?? 'en') : null}
			onsentence={sentences ? widenToSentences : null}
			onhighlight={saveHighlight}
			onsave={saveNote}
			ondelete={deleteAnnotation}
			onshare={editing ? shareAnnotation : null}
			oncancel={dismissAnnotate}
		/>
	{/if}


	{#if speechState !== 'off'}
		<SpeechBar
			state={speechState}
			prefs={speechPrefs}
			language={speechLanguage}
			voices={speechVoices}
			raised={chromeVisible}
			ontoggle={toggleSpeech}
			onstep={(direction) => speech?.step(direction)}
			onprefs={changeSpeechPrefs}
			onstop={() => speech?.stop()}
		/>
	{/if}

	{#if status === 'loading'}
		<div class="notice" role="status">
			{#if coverUrl}
				<img class="cover" src={coverUrl} alt="" />
			{/if}
			<p class="title">{book.title}</p>
			{#if book.author}<p class="author">{book.author}</p>{/if}
			<div class="bar" class:indeterminate={downloaded === null || downloaded >= 1}>
				<span style:width={`${(downloaded ?? 0) * 100}%`}></span>
			</div>
			<p class="what">{t('reader.loading')}</p>
		</div>
	{:else if status === 'error'}
		<div class="notice" role="alert">
			<TriangleAlert size={28} />
			<p class="message">{errorText}</p>
			<div class="notice-actions">
				{#if errorCode === 'network'}
					<button class="solid" onclick={open}>{t('reader.retry')}</button>
				{/if}
				<a href={backHref} onclick={leave}>{t('reader.back')}</a>
			</div>
		</div>
	{/if}

	{#if hint && status === 'ready' && paginated}
		<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
		<div class="zones" onclick={dismissHint}>
			<div><ChevronLeft size={26} />{t('reader.hint.prev')}</div>
			<div class="middle">{t('reader.hint.menu')}</div>
			<div>{t('reader.hint.next')}<ChevronRight size={26} /></div>
		</div>
	{/if}

	<ProgressBar
		{location}
		ready={locationsReady}
		expanded={chromeVisible && status === 'ready'}
		{paginated}
		{rtl}
		chapterAt={(fraction) => engine?.chapterAt(fraction) ?? null}
		canReturn={returnTo !== null}
		onreturn={goBack}
		pace={book.reading_time?.pace ?? null}
		bookChars={book.reading_time?.chars ?? null}
		onseek={seek}
		onprev={() => turn(false)}
		onnext={() => turn(true)}
	/>

	<TocPanel
		open={panel === 'toc'}
		{toc}
		currentId={location?.tocId ?? null}
		onselect={goTo}
		onclose={closePanel}
	/>
	<SearchPanel
		open={panel === 'search'}
		query={searchQuery}
		hits={searchHits}
		{searching}
		{searched}
		truncated={searchTruncated}
		currentCfi={searchCurrent}
		onsearch={search}
		onselect={goToHit}
		onclose={closePanel}
	/>
	<SettingsPanel
		open={panel === 'settings'}
		{settings}
		onchange={changeSettings}
		onclose={closePanel}
	/>
	<HelpPanel open={panel === 'help'} fullscreen={fullscreen !== null} speech={canSpeak} onclose={closePanel} />
	<NotesPanel
		open={panel === 'notes'}
		{annotations}
		{exportHref}
		onselect={goToAnnotation}
		onedit={editAnnotation}
		onclose={closePanel}
	/>
</div>

<!-- Outside the reader, so that it has the catalog's own look. -->
<ShareQuote
	open={sharing !== null}
	quote={sharing?.text ?? ''}
	note={sharing?.note ?? null}
	title={book.title}
	author={book.author}
	{coverUrl}
	color={sharing?.color}
	onclose={() => (sharing = null)}
/>

<style>
	.reader {
		position: fixed;
		top: 0;
		left: 0;
		width: 100%;
		height: 100%;
		/* Follows the browser's toolbars as they come and go on phones. */
		height: 100dvh;
		display: grid;
		grid-template-rows: minmax(0, 1fr) auto;
		/* One column the width of the window, whatever its rows hold: without
		   it a long chapter title in the status line widens the column, and
		   the page with it, past the edge of a phone. */
		grid-template-columns: minmax(0, 1fr);
		overflow: hidden;
	}
	/* The margins above and below the page. They scale with the window, so a
	   phone in landscape keeps its few lines and a tall window gets air. The
	   top one also holds the notch, and is where the toolbar slides in, so on
	   most screens the toolbar covers margin rather than text. */
	.stage {
		min-width: 0;
		min-height: 0;
		overflow: hidden;
		padding: max(env(safe-area-inset-top), clamp(0.5rem, 3.5vh, 2.75rem)) env(safe-area-inset-right)
			clamp(0rem, 1.5vh, 1.25rem) env(safe-area-inset-left);
		outline: none;
		/* A swipe turns the page; nothing here pans. Pinching still zooms. */
		touch-action: pinch-zoom;
		-webkit-user-select: none;
		user-select: none;
	}
	.stage.scrolled {
		padding-top: env(safe-area-inset-top);
		padding-bottom: 0;
		touch-action: pan-y pinch-zoom;
	}
	.stage.busy {
		visibility: hidden;
	}
	/* With motion turned on for the catalog, the page comes in once it has
	   been laid out, instead of standing there all at once. */
	:global(:root[data-motion='on']) .stage:not(.busy) {
		animation: arrive 0.55s cubic-bezier(0.2, 0.7, 0.2, 1);
	}
	@keyframes arrive {
		from {
			opacity: 0;
			transform: translateY(10px);
		}
	}
	/* The ends of a passage being picked: a bar the height of the line, with
	   a knob above the start and below the end, and room around the knob for
	   a finger. */
	.handle {
		position: absolute;
		z-index: 2;
		width: 2px;
		margin-left: -1px;
		background: var(--r-link);
		touch-action: none;
		user-select: none;
		-webkit-user-select: none;
		-webkit-touch-callout: none;
		cursor: grab;
	}
	.handle::before {
		content: '';
		position: absolute;
		left: 50%;
		width: 12px;
		height: 12px;
		margin-left: -6px;
		border-radius: 50%;
		background: var(--r-link);
	}
	.handle.start::before {
		top: -11px;
	}
	.handle.end::before {
		bottom: -11px;
	}
	.handle::after {
		content: '';
		position: absolute;
		left: 50%;
		width: 44px;
		height: calc(100% + 32px);
		margin-left: -22px;
	}
	.handle.start::after {
		top: -32px;
	}
	.handle.end::after {
		top: 0;
	}
	.stage.shield :global(iframe) {
		pointer-events: none;
	}
	/* Over the shield the pointer is the text cursor it would be over the
	   text itself, so that it looks like text that can be picked. */
	@media (pointer: fine) {
		.stage.shield {
			cursor: text;
		}
	}
	.page {
		width: 100%;
		height: 100%;
	}
	/* Two names for the same animation: switching between them restarts it. */
	.page.slide-a {
		animation: slide-in-a 0.18s ease-out;
	}
	.page.slide-b {
		animation: slide-in-b 0.18s ease-out;
	}
	.page.back {
		--slide-from: -2.5%;
	}
	@keyframes slide-in-a {
		from {
			opacity: 0.25;
			translate: var(--slide-from, 2.5%) 0;
		}
	}
	@keyframes slide-in-b {
		from {
			opacity: 0.25;
			translate: var(--slide-from, 2.5%) 0;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.page.slide-a,
		.page.slide-b {
			animation: none;
		}
	}

	.edge {
		position: absolute;
		top: 50%;
		z-index: 2;
		display: grid;
		place-items: center;
		width: 2.75rem;
		height: 4.5rem;
		margin-top: -2.25rem;
		border-radius: 12px;
		color: var(--r-muted);
		background: color-mix(in srgb, var(--r-fg) 6%, transparent);
		opacity: 0;
		pointer-events: none;
		transition: opacity 0.15s ease;
	}
	.edge.start {
		left: 0.5rem;
	}
	.edge.end {
		right: 0.5rem;
	}
	.edge.on {
		opacity: 1;
	}

	/* The note of the highlight under the mouse. */
	.note-tip {
		position: fixed;
		z-index: 5;
		max-width: min(22rem, calc(100vw - 2rem));
		padding: 0.5rem 0.7rem;
		border-radius: 8px;
		background: var(--r-surface);
		color: var(--r-fg);
		border: 1px solid var(--r-border);
		box-shadow: 0 2px 14px rgba(0, 0, 0, 0.16);
		font-size: 0.86rem;
		line-height: 1.4;
		white-space: pre-wrap;
		pointer-events: none;
	}
	.note-tip.above {
		transform: translateY(calc(-100% - 2rem));
	}
	.pill {
		position: absolute;
		z-index: 3;
		left: 50%;
		bottom: calc(2.9rem + env(safe-area-inset-bottom));
		transform: translateX(-50%);
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		min-height: 2.75rem;
		padding: 0 1.1rem;
		border-radius: 99px;
		background: var(--r-surface);
		color: var(--r-fg);
		border: 1px solid var(--r-border);
		box-shadow: 0 2px 14px rgba(0, 0, 0, 0.16);
		white-space: nowrap;
		animation: rise 0.2s ease-out;
	}
	.reader button.pill {
		padding: 0 1.1rem;
		border-radius: 99px;
		background: var(--r-surface);
		border-color: var(--r-border);
	}
	.pill.toast {
		pointer-events: none;
	}
	/* Above the bar for reading aloud. */
	.pill.toast.lifted {
		bottom: calc(9.2rem + env(safe-area-inset-bottom));
	}
	@keyframes rise {
		from {
			opacity: 0;
			translate: 0 0.5rem;
		}
	}

	.notice {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		padding: 2rem;
		text-align: center;
		color: var(--r-muted);
	}
	.notice p {
		margin: 0;
		max-width: 28rem;
	}
	.notice .title {
		color: var(--r-fg);
		font-family: 'Fraunces', Georgia, serif;
		font-size: 1.15rem;
		font-weight: 600;
	}
	.notice .message {
		color: var(--r-fg);
		font-size: 1.02rem;
	}
	.notice .what {
		font-size: 0.82rem;
	}
	.cover {
		width: auto;
		max-width: 9rem;
		max-height: min(13rem, 34vh);
		margin-bottom: 0.75rem;
		border-radius: 4px;
		box-shadow:
			0 1px 2px rgba(0, 0, 0, 0.2),
			0 8px 24px rgba(0, 0, 0, 0.18);
	}
	.bar {
		width: min(12rem, 60vw);
		height: 3px;
		margin: 1rem 0 0.25rem;
		border-radius: 99px;
		overflow: hidden;
		background: color-mix(in srgb, var(--r-fg) 12%, transparent);
	}
	.bar span {
		display: block;
		height: 100%;
		border-radius: inherit;
		background: var(--r-link);
		transition: width 0.15s linear;
	}
	.bar.indeterminate span {
		width: 40% !important;
		transition: none;
		animation: slide 1.1s ease-in-out infinite;
	}
	@keyframes slide {
		from {
			transform: translateX(-100%);
		}
		to {
			transform: translateX(250%);
		}
	}
	.notice-actions {
		display: flex;
		align-items: center;
		gap: 1rem;
		margin-top: 0.75rem;
	}
	.notice a {
		display: inline-flex;
		align-items: center;
		min-height: 2.75rem;
		color: var(--r-link);
	}
	.reader button.solid {
		min-height: 2.75rem;
		padding: 0 1.1rem;
		background: var(--r-link);
		color: var(--r-bg);
	}

	.zones {
		position: absolute;
		inset: 0;
		z-index: 5;
		display: grid;
		grid-template-columns: 1fr 1fr 1fr;
		background: color-mix(in srgb, var(--r-fg) 90%, transparent);
		color: var(--r-bg);
		font-weight: 600;
		text-align: center;
		animation: fade 0.25s ease-out;
	}
	.zones div {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.15rem;
		padding: 0.5rem;
	}
	.zones .middle {
		border-left: 1px dashed color-mix(in srgb, var(--r-bg) 50%, transparent);
		border-right: 1px dashed color-mix(in srgb, var(--r-bg) 50%, transparent);
	}
	@keyframes fade {
		from {
			opacity: 0;
		}
	}
</style>
