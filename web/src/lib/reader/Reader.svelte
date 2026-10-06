<script lang="ts">
	import './reader.css';
	import { onMount, tick } from 'svelte';
	import { goto, invalidateAll } from '$app/navigation';
	import { ChevronLeft, ChevronRight, ChevronsRight, TriangleAlert } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import type { BookDetail } from '#lib/types';
	import { openReader } from './engine';
	import {
		attachGestures,
		followLinkAt,
		frameEventsWork,
		linkAt,
		type GestureHandlers
	} from './gestures';
	import { createProgressStore } from './progress';
	import { LIMITS, loadSettings, saveSettings } from './settings';
	import { palettes } from './themes';
	import {
		ReaderError,
		type ReaderEngine,
		type ReaderErrorCode,
		type ReaderLocation,
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
	import HelpPanel from './HelpPanel.svelte';
	import SpeechBar from './SpeechBar.svelte';
	import ProgressBar from './ProgressBar.svelte';
	import ReaderToolbar from './ReaderToolbar.svelte';
	import SearchPanel from './SearchPanel.svelte';
	import SettingsPanel from './SettingsPanel.svelte';
	import TocPanel from './TocPanel.svelte';

	// One reader per book: the page keys this component on the book's id.
	let { book }: { book: BookDetail } = $props();
	// svelte-ignore state_referenced_locally
	const bookId = book.id;
	const backHref = `/books/${bookId}`;

	type Panel = 'toc' | 'settings' | 'search' | 'help';

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
	// Positions are only saved once the reader has moved: the position a book
	// opens at may come from another device and must not be written back.
	let moved = false;
	let savedCfi: string | null = null;
	let savedExact = false;

	function onRelocated(next: ReaderLocation) {
		location = next;
		// The reader turned the page or jumped: reading aloud goes on from there.
		speech?.resync();
		if (!moved) return;
		// The same place is saved again once its percentage is exact: until the
		// positions are worked out it is an estimate, and the catalog shows it.
		if (next.cfi === savedCfi && (savedExact || !next.exact)) return;
		savedCfi = next.cfi;
		savedExact = next.exact;
		progress.save({ cfi: next.cfi, percent: next.percent });
	}

	/**
	 * Leave for the book's page, once the position has reached the server:
	 * that page shows the progress, and would otherwise load the old one.
	 */
	async function leave(event: MouseEvent) {
		if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
		event.preventDefault();
		await Promise.race([progress.flush(), new Promise((resolve) => setTimeout(resolve, 800))]);
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

	function turn(forward: boolean) {
		if (!engine) return;
		if (forward && location?.atEnd) {
			showToast(t('reader.endOfBook'));
			return;
		}
		moved = true;
		hideChrome();
		clearHighlight();
		// Reading on from where a jump landed: the way back is no longer wanted.
		if (returnTo && ++turnsSinceJump >= 3) forgetReturn();
		(forward ? engine.next() : engine.prev()).then(() => slide(forward)).catch(() => {});
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

	// ---- Input ------------------------------------------------------------------

	let wheelAt = 0;
	const gestures: GestureHandlers = {
		area: () => host.getBoundingClientRect(),
		onTap(zone, point) {
			if (hint) return dismissHint();
			if (shield && linkAt(host, point.x, point.y)) {
				leaving();
				if (!followLinkAt(host, point.x, point.y)) forgetReturn();
				return;
			}
			if (!paginated || zone === 'center') {
				if (chrome) hideChrome();
				else showChrome();
			} else turn((zone === 'end') !== rtl);
		},
		onSwipe(forward) {
			if (hint) dismissHint();
			if (paginated) turn(forward !== rtl);
		},
		onKey: handleKey,
		onLink: leaving,
		onPointer: pointerAt,
		onWheel(deltaY) {
			// One page per wheel gesture; trackpads keep sending events while coasting.
			if (!paginated || Math.abs(deltaY) < 20 || Date.now() - wheelAt < 450) return;
			wheelAt = Date.now();
			turn(deltaY > 0);
		},
		onActivity() {
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
			// With the shield up the browser no longer shows what is a link.
			if (shield) stage.style.cursor = inPage && linkAt(host, point.x, point.y) ? 'pointer' : '';
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
		if (event.defaultPrevented) return;
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
		if ((next === 'toc' || next === 'search') && !engine) return;
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

	async function open() {
		status = 'loading';
		downloaded = null;
		try {
			const saved = await progress.load();
			if (closed) return;
			savedCfi = saved?.cfi ?? null;
			const opened = await openReader({
				host,
				url: `/api/books/${bookId}/file`,
				cacheKey: String(bookId),
				cacheVersion: `${book.file_size}:${book.updated_at ?? book.created_at}`,
				settings,
				initialCfi: saved?.cfi,
				initialPercent: saved?.percent,
				onDownload: (fraction) => (downloaded = fraction),
				onRelocated,
				onLocationsReady: () => (locationsReady = true),
				onContent(doc) {
					if (shield) return;
					if (forceShield || !frameEventsWork(doc)) shield = true;
					else attachGestures(doc, gestures);
				}
			});
			if (closed) {
				opened.destroy();
				return;
			}
			engine = opened;
			toc = opened.toc;
			location = opened.location;
			locationsReady = opened.locationsReady;
			status = 'ready';
			// Settings may have been changed while the book was loading.
			opened.applySettings(settings).catch(() => {});
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
		const flush = () => progress.flush();
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
		host.addEventListener('pointerdown', onPointerDown);
		stage.addEventListener('focusin', onFocusIn);

		open();

		return () => {
			closed = true;
			document.removeEventListener('fullscreenchange', onFullscreen);
			document.removeEventListener('visibilitychange', onVisibility);
			window.removeEventListener('pagehide', flush);
			host?.removeEventListener('pointerdown', onPointerDown);
			stage?.removeEventListener('focusin', onFocusIn);
			for (const timer of [chromeTimer, returnTimer, toastTimer]) if (timer) clearTimeout(timer);
			if (pointerFrame) cancelAnimationFrame(pointerFrame);
			searchAbort?.abort();
			speech?.stop();
			if (canSpeak) speechSynthesis.removeEventListener('voiceschanged', onVoices);
			flush();
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

<svelte:window onkeydown={onWindowKey} onpointermove={onWindowPointerMove} />

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
</div>

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
	.stage.shield :global(iframe) {
		pointer-events: none;
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
