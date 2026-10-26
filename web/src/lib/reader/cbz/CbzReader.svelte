<script lang="ts">
	// A reader for comic book archives (CBZ): a page at a time fitted to the
	// screen, or two side by side on a wide one, turned with a tap at the side,
	// a swipe, the arrow keys or the slider. Manga (ComicInfo.xml says so) is
	// read from the right. Two fingers, a double tap, Ctrl and the wheel or
	// the buttons zoom in, and a zoomed page is moved about by dragging. The
	// archive is read whole and its pages unpacked as they come near.
	import { onMount, tick } from 'svelte';
	import { goto } from '$app/navigation';
	import { ArrowLeft, ChevronLeft, ChevronRight, Minus, Moon, Plus, Sun, SunMoon } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import type { BookDetail } from '#lib/types';
	import { createProgressStore } from '../progress';
	import { startReadingClock } from '../readingClock';
	import { loadSettings, saveSettings, THEMES } from '../settings';
	import { palettes } from '../themes';
	import type { ReaderTheme } from '../types';
	import '../reader.css';
	import { comicContents, encodePage, pageOf, readsRightToLeft, spreads, type ComicPage } from './comic';
	import { readEntries, readEntry } from './zip';

	type Props = { book: BookDetail; back: string | null };
	let { book, back }: Props = $props();

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

	// ---- The archive ------------------------------------------------------------

	let status = $state<'loading' | 'ready' | 'error'>('loading');
	let failure = $state('');
	let downloaded = $state<number | null>(null);
	let data: ArrayBuffer | null = null;
	let pages: ComicPage[] = [];
	let total = $state(0);
	let rtl = $state(false);
	/** Each page's picture, as an object URL, while it is near. */
	let urls = $state.raw<Map<number, string>>(new Map());
	/** Pages known to be wider than tall: a picture over two pages, shown alone. */
	let wide = $state.raw<Set<number>>(new Set());
	const unpacking = new Set<number>();
	/** How many pages either side of the ones on screen are kept unpacked. */
	const NEAR = 4;

	async function unpack(n: number) {
		if (!data || urls.has(n) || unpacking.has(n) || n < 1 || n > total) return;
		unpacking.add(n);
		try {
			const page = pages[n - 1];
			const bytes = await readEntry(data, page.entry);
			if (!unpacking.has(n)) return;
			urls = new Map(urls).set(n, URL.createObjectURL(new Blob([bytes], { type: page.type })));
		} catch {
			// A page that does not unpack stays empty; the others still read.
		} finally {
			unpacking.delete(n);
		}
	}

	/** Unpack the pages near those on screen and let go of the far ones. */
	function keepNear() {
		const shown = view;
		if (!shown.length) return;
		const from = shown[0] - NEAR * 2;
		const to = shown[shown.length - 1] + NEAR * 2;
		for (let n = shown[0]; n <= to; n++) void unpack(n);
		for (let n = shown[0] - 1; n >= from; n--) void unpack(n);
		const far = [...urls.keys()].filter((n) => n < from - NEAR || n > to + NEAR);
		if (far.length) {
			const next = new Map(urls);
			for (const n of far) {
				URL.revokeObjectURL(next.get(n)!);
				next.delete(n);
			}
			urls = next;
		}
	}

	function onLoad(n: number, event: Event) {
		const img = event.currentTarget as HTMLImageElement;
		if (img.naturalWidth > img.naturalHeight * 1.05 && !wide.has(n)) wide = new Set(wide).add(n);
	}

	// ---- Layout -----------------------------------------------------------------

	let stage = $state<HTMLDivElement | null>(null);
	let width = $state(0);
	let height = $state(0);
	/** Two pages side by side on a screen wide enough and wider than tall. */
	const twoUp = $derived(width >= 900 && width > height * 1.15);
	const pairs = $derived(spreads(total, twoUp, (n) => wide.has(n)));
	/** The first page on screen; the spread follows from it. */
	let page = $state(1);
	const at = $derived(Math.max(0, pairs.findIndex((p) => p.includes(page))));
	const view = $derived(pairs[at] ?? []);

	$effect(() => {
		void view;
		void urls;
		if (status === 'ready') keepNear();
	});

	// ---- Position ---------------------------------------------------------------

	const progress = createProgressStore(bookId);
	const clock = startReadingClock(bookId, () => (status === 'ready' ? 'reading' : 'away'));

	function save() {
		if (!total || !view.length) return;
		progress.save({ cfi: encodePage(view[0]), percent: view[view.length - 1] / total });
	}

	/** Show the spread with this page; the reader asked, so it is saved. */
	function go(n: number) {
		const next = Math.min(Math.max(n, 1), total);
		if (next === page && view.includes(next)) return;
		page = next;
		resetZoom();
		chrome = false;
		tick().then(save);
	}

	/** One spread on, or back; `forward` in reading order. */
	function turn(forward: boolean) {
		clock.touch();
		const next = pairs[at + (forward ? 1 : -1)];
		if (next) go(next[0]);
	}

	/** A turn towards a side of the screen: on in a book read from the left, back in manga. */
	const turnTo = (side: 'left' | 'right') => turn((side === 'right') !== rtl);

	// ---- Zoom -------------------------------------------------------------------

	let zoom = $state(1);
	const ZOOM_MAX = 5;

	function resetZoom() {
		zoom = 1;
		stage?.scrollTo({ left: 0, top: 0 });
	}

	/** Set the zoom, keeping the point at (x, y) of the stage where it is. */
	function setZoom(next: number, x = width / 2, y = height / 2) {
		if (!stage) return;
		const z = Math.min(ZOOM_MAX, Math.max(1, Math.round(next * 100) / 100));
		if (z === zoom) return;
		const ratio = z / zoom;
		const left = (stage.scrollLeft + x) * ratio - x;
		const top = (stage.scrollTop + y) * ratio - y;
		zoom = z;
		tick().then(() => {
			if (!stage) return;
			stage.scrollLeft = left;
			stage.scrollTop = top;
		});
	}

	function onWheel(event: WheelEvent) {
		if (!event.ctrlKey && !event.metaKey) return;
		event.preventDefault();
		const box = stage!.getBoundingClientRect();
		setZoom(zoom * Math.exp(-event.deltaY / 300), event.clientX - box.left, event.clientY - box.top);
	}

	// ---- Touch and taps -----------------------------------------------------------

	// One finger: a swipe turns the page (at full size). Two: the page scales
	// as a picture while they move, and settles at the new zoom when lifted.
	let swipe: { x: number; y: number; time: number } | null = null;
	let pinch: { distance: number; x: number; y: number; zoom: number } | null = null;
	let pinchScale = $state(1);
	let pinchOrigin = $state('0 0');
	/** When a swipe or pinch last ended: the click that follows it is not a tap. */
	let gestured = 0;

	const fingers = (e: TouchEvent) => Math.hypot(e.touches[0].clientX - e.touches[1].clientX, e.touches[0].clientY - e.touches[1].clientY);

	function onTouchStart(e: TouchEvent) {
		clock.touch();
		if (e.touches.length === 1) {
			swipe = { x: e.touches[0].clientX, y: e.touches[0].clientY, time: Date.now() };
		} else if (e.touches.length === 2 && stage) {
			swipe = null;
			const box = stage.getBoundingClientRect();
			const x = (e.touches[0].clientX + e.touches[1].clientX) / 2 - box.left;
			const y = (e.touches[0].clientY + e.touches[1].clientY) / 2 - box.top;
			pinch = { distance: fingers(e), x, y, zoom };
			pinchOrigin = `${stage.scrollLeft + x}px ${stage.scrollTop + y}px`;
		}
	}

	function onTouchMove(e: TouchEvent) {
		if (!pinch || e.touches.length !== 2) return;
		e.preventDefault();
		const wanted = Math.min(ZOOM_MAX, Math.max(1, (pinch.zoom * fingers(e)) / pinch.distance));
		pinchScale = wanted / pinch.zoom;
	}

	function onTouchEnd(e: TouchEvent) {
		if (pinch && e.touches.length < 2) {
			const done = pinch;
			pinch = null;
			const scale = pinchScale;
			pinchScale = 1;
			gestured = Date.now();
			if (Math.abs(scale - 1) > 0.02) setZoom(done.zoom * scale, done.x, done.y);
			return;
		}
		if (!swipe || zoom > 1 || e.changedTouches.length !== 1) return;
		const dx = e.changedTouches[0].clientX - swipe.x;
		const dy = e.changedTouches[0].clientY - swipe.y;
		const quick = Date.now() - swipe.time < 800;
		swipe = null;
		if (quick && Math.abs(dx) > 50 && Math.abs(dx) > Math.abs(dy) * 1.5) {
			gestured = Date.now();
			// Pulling the page to the left brings the one to its right.
			turnTo(dx < 0 ? 'right' : 'left');
		}
	}

	let chrome = $state(true);
	/** A tap in the middle waits a moment for a second tap, which zooms. */
	let tapTimer: ReturnType<typeof setTimeout> | null = null;
	const DOUBLE_TAP_MS = 260;

	function onTap(event: MouseEvent) {
		if (Date.now() - gestured < 400) return;
		if (!stage || status !== 'ready') return;
		const box = stage.getBoundingClientRect();
		const x = event.clientX - box.left;
		const y = event.clientY - box.top;
		const side = x < box.width * 0.3 ? 'left' : x > box.width * 0.7 ? 'right' : null;
		// At full size the sides turn at once; zoomed in, a tap anywhere waits for a second.
		if (side && zoom === 1 && !tapTimer) {
			turnTo(side);
			return;
		}
		if (tapTimer) {
			clearTimeout(tapTimer);
			tapTimer = null;
			if (zoom > 1) resetZoom();
			else setZoom(2.5, x, y);
			return;
		}
		tapTimer = setTimeout(() => {
			tapTimer = null;
			chrome = !chrome;
		}, DOUBLE_TAP_MS);
	}

	function onKey(event: KeyboardEvent) {
		const el = event.target as HTMLElement | null;
		if (el?.closest('input, textarea, select, dialog')) return;
		if (event.metaKey || event.ctrlKey || event.altKey || status !== 'ready') return;
		clock.touch();
		const keys: Record<string, () => void> = {
			ArrowRight: () => turnTo('right'),
			ArrowLeft: () => turnTo('left'),
			PageDown: () => turn(true),
			PageUp: () => turn(false),
			' ': () => turn(!event.shiftKey),
			Home: () => go(1),
			End: () => go(total),
			'+': () => setZoom(zoom * 1.25),
			'=': () => setZoom(zoom * 1.25),
			'-': () => setZoom(zoom / 1.25),
			'0': resetZoom
		};
		const action = keys[event.key];
		if (!action) return;
		event.preventDefault();
		action();
	}

	async function leave(event: MouseEvent) {
		if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
		event.preventDefault();
		await Promise.race([Promise.all([progress.flush(), clock.flush()]), new Promise((resolve) => setTimeout(resolve, 800))]);
		goto(backHref);
	}

	// ---- Opening ----------------------------------------------------------------

	/** The archive, with how much of it has come. */
	async function download(): Promise<ArrayBuffer> {
		const res = await fetch(fileUrl);
		if (!res.ok) throw new Error(res.status === 404 ? 'notFound' : 'network');
		const size = Number(res.headers.get('content-length')) || 0;
		if (!res.body || !size) return await res.arrayBuffer();
		const chunks: Uint8Array<ArrayBuffer>[] = [];
		const reader = res.body.getReader();
		let loaded = 0;
		for (;;) {
			const { done, value } = await reader.read();
			if (done) break;
			chunks.push(value);
			loaded += value.length;
			downloaded = Math.min(1, loaded / size);
		}
		return await new Blob(chunks).arrayBuffer();
	}

	onMount(() => {
		const html = document.documentElement;
		html.classList.add('legejo-reading');
		const resize = new ResizeObserver(() => {
			width = stage?.clientWidth ?? 0;
			height = stage?.clientHeight ?? 0;
		});
		if (stage) resize.observe(stage);
		const flush = () => {
			progress.flush();
			clock.flush();
		};
		const onVisibility = () => {
			if (document.visibilityState === 'hidden') flush();
		};
		document.addEventListener('visibilitychange', onVisibility);
		window.addEventListener('pagehide', flush);
		let closed = false;

		(async () => {
			try {
				const [saved, archive] = await Promise.all([progress.load(), download()]);
				if (closed) return;
				let entries;
				try {
					entries = readEntries(archive);
				} catch {
					throw new Error('invalid');
				}
				const contents = comicContents(entries);
				if (!contents.pages.length) throw new Error('invalid');
				data = archive;
				pages = contents.pages;
				if (contents.info) {
					const xml = new TextDecoder().decode(await readEntry(archive, contents.info).catch(() => new Uint8Array()));
					rtl = readsRightToLeft(xml);
				}
				total = pages.length;
				if (saved) page = Math.min(total, pageOf(saved.cfi) ?? Math.floor((saved.percent ?? 0) * total) + 1);
				status = 'ready';
			} catch (e) {
				status = 'error';
				const reason = (e as Error)?.message;
				failure = reason === 'notFound' ? t('reader.error.notFound') : reason === 'invalid' ? t('reader.comic.invalid') : t('reader.error.network');
			}
		})();

		return () => {
			closed = true;
			flush();
			clock.stop();
			resize.disconnect();
			if (tapTimer) clearTimeout(tapTimer);
			document.removeEventListener('visibilitychange', onVisibility);
			window.removeEventListener('pagehide', flush);
			unpacking.clear();
			for (const url of urls.values()) URL.revokeObjectURL(url);
			data = null;
			html.classList.remove('legejo-reading');
		};
	});

	const where = $derived(view.length > 1 ? `${view[0]}–${view[1]}` : String(view[0] ?? 1));
</script>

<svelte:window onkeydown={onKey} />

<div
	class="reader cbz"
	style:--r-bg={palette.bg}
	style:--r-fg={palette.fg}
	style:--r-muted={palette.muted}
	style:--r-link={palette.link}
	style:--r-surface={palette.surface}
	style:--r-border={palette.border}
	data-theme={settings.theme}
>
	<header class="bar top" class:hidden={!chrome}>
		<a class="r-icon" href={backHref} onclick={leave} aria-label={t('reader.back')} title={t('reader.back')}>
			<ArrowLeft size={20} />
		</a>
		<div class="heading">
			<h1>{book.title}</h1>
			{#if book.author}<p>{book.author}</p>{/if}
		</div>
		<button class="r-icon" onclick={nextTheme} aria-label={t('reader.theme')} title={t('reader.theme')}>
			<ThemeIcon size={19} />
		</button>
	</header>

	<!-- The pages. Taps and swipes turn them here; the bars are the accessible way to the same. -->
	<div
		class="stage"
		class:zoomed={zoom > 1}
		bind:this={stage}
		onwheel={onWheel}
		ontouchstart={onTouchStart}
		ontouchmove={onTouchMove}
		ontouchend={onTouchEnd}
		ontouchcancel={onTouchEnd}
		onclick={onTap}
		role="presentation"
	>
		{#if status === 'ready'}
			<div
				class="spread"
				class:rtl
				style:width={`${zoom * 100}%`}
				style:height={`${zoom * 100}%`}
				style:transform={pinchScale !== 1 ? `scale(${pinchScale})` : null}
				style:transform-origin={pinchOrigin}
			>
				{#each view as n (n)}
					<div class="page" class:half={view.length > 1}>
						{#if urls.get(n)}
							<img src={urls.get(n)} alt={t('reader.comic.page', { page: n })} draggable="false" onload={(e) => onLoad(n, e)} />
						{/if}
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
			<button class="r-icon" onclick={() => turnTo('left')} disabled={rtl ? at >= pairs.length - 1 : at === 0} aria-label={rtl ? t('reader.nextPage') : t('reader.prevPage')} title={rtl ? t('reader.nextPage') : t('reader.prevPage')}>
				<ChevronLeft size={20} />
			</button>
			<input
				type="range"
				min="1"
				max={total}
				value={view[0] ?? 1}
				dir={rtl ? 'rtl' : 'ltr'}
				aria-label={t('reader.position')}
				onchange={(e) => go(Number(e.currentTarget.value))}
			/>
			<button class="r-icon" onclick={() => turnTo('right')} disabled={rtl ? at === 0 : at >= pairs.length - 1} aria-label={rtl ? t('reader.prevPage') : t('reader.nextPage')} title={rtl ? t('reader.prevPage') : t('reader.nextPage')}>
				<ChevronRight size={20} />
			</button>
			<span class="where">{t('reader.pdf.pageOf', { page: where, total })}</span>
			<span class="zoom">
				<button class="r-icon" onclick={() => setZoom(zoom / 1.25)} disabled={zoom <= 1} aria-label={t('reader.pdf.zoomOut')} title={t('reader.pdf.zoomOut')}>
					<Minus size={18} />
				</button>
				<button class="fit" onclick={resetZoom} disabled={zoom === 1} title={t('reader.comic.fit')}>
					{Math.round(zoom * 100)} %
				</button>
				<button class="r-icon" onclick={() => setZoom(zoom * 1.25)} disabled={zoom >= ZOOM_MAX} aria-label={t('reader.pdf.zoomIn')} title={t('reader.pdf.zoomIn')}>
					<Plus size={18} />
				</button>
			</span>
		</footer>
	{/if}
</div>

<style>
	.reader.cbz {
		position: fixed;
		inset: 0;
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
	.zoom {
		display: flex;
		align-items: center;
	}
	.fit {
		min-width: 3.4rem;
		font-size: 0.82rem;
		color: var(--r-fg);
		font-variant-numeric: tabular-nums;
	}
	/* The zoom buttons on a phone: two fingers and a double tap are there. */
	@media (max-width: 560px) {
		.zoom {
			display: none;
		}
	}

	.stage {
		position: absolute;
		inset: 0;
		overflow: hidden;
		overscroll-behavior: contain;
		/* Swipes and pinches are the reader's own at full size. */
		touch-action: none;
		user-select: none;
		-webkit-user-select: none;
		-webkit-touch-callout: none;
	}
	.stage.zoomed {
		overflow: auto;
		touch-action: pan-x pan-y;
	}
	.spread {
		display: flex;
		align-items: center;
		justify-content: center;
		min-width: 100%;
		min-height: 100%;
	}
	.spread.rtl {
		flex-direction: row-reverse;
	}
	.page {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		max-width: 100%;
	}
	/* Two pages meet at the spine: each leans towards the other. */
	.page.half {
		max-width: 50%;
	}
	.spread:not(.rtl) .page.half:first-child,
	.spread.rtl .page.half:last-child {
		justify-content: flex-end;
	}
	.spread:not(.rtl) .page.half:last-child,
	.spread.rtl .page.half:first-child {
		justify-content: flex-start;
	}
	.page img {
		display: block;
		max-width: 100%;
		max-height: 100%;
		object-fit: contain;
		box-shadow: 0 1px 6px rgb(0 0 0 / 0.18);
	}

	.notice {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
		height: 100%;
		padding: 2rem;
		text-align: center;
	}
	.notice .title {
		font-weight: 600;
	}
	.progressbar {
		width: min(16rem, 70vw);
		height: 4px;
		border-radius: 2px;
		background: var(--r-border);
		overflow: hidden;
	}
	.progressbar span {
		display: block;
		height: 100%;
		background: var(--r-link);
	}
	.progressbar.indeterminate span {
		width: 30% !important;
		animation: slide 1.2s ease-in-out infinite;
	}
	@keyframes slide {
		from {
			translate: -100% 0;
		}
		to {
			translate: 350% 0;
		}
	}
</style>
