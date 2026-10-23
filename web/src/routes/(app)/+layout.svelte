<script lang="ts">
	import { afterNavigate, beforeNavigate, goto, invalidateAll } from '$app/navigation';
	import { dev } from '$app/env';
	import { page, updated } from '$app/state';
	import { onMount } from 'svelte';
	import {
		BookDown,
		BookMarked,
		BookOpenCheck,
		Earth,
		Ellipsis,
		Feather,
		Globe,
		HardDriveDownload,
		Headphones,
		LibraryBig,
		LogOut,
		Search,
		Settings,
		Sparkles,
		Users,
		X,
		Highlighter
	} from '@lucide/svelte';
	import Avatar from '#lib/Avatar.svelte';
	import Logo from '#lib/Logo.svelte';
	import WhatsNew from '#lib/WhatsNew.svelte';
	import { newBookCount, refreshFollowNews } from '#lib/follows.svelte';
	import { t } from '#lib/i18n';
	import { CHANGED_EVENT, availability, forgetEverything, savedBooks, warmSession } from '#lib/offline/store';
	import { sendUnsent } from '#lib/reader/progress';
	import { sendUnsentTime } from '#lib/reader/readingClock';
	import type { LayoutData } from './$types';

	let { data, children }: { data: LayoutData; children: any } = $props();

	// On a phone the navigation is a row of tabs at the foot of the screen,
	// in reach of a thumb; two of them open a sheet from below with the rest
	// of what the sidebar holds on a wide screen.
	let sheet = $state<'shelves' | 'more' | null>(null);
	afterNavigate(() => (sheet = null));

	/** The tab the page belongs to, or the one whose sheet is open. */
	const tab = $derived.by(() => {
		if (sheet) return sheet;
		const path = page.url.pathname;
		if (path === '/' || path.startsWith('/books/')) return 'library';
		if (path.startsWith('/reading')) return 'reading';
		if (path.startsWith('/search')) return 'search';
		if (/^\/(shelves|public|linked|fediverse|catalogs)(\/|$)/.test(path)) return 'shelves';
		return 'more';
	});

	/** The pages the tabs lead to, whose heading has the pigeon beside it on a phone. */
	const TAB_PAGES = ['/', '/reading', '/search', '/notes', '/authors', '/audiobooks', '/public'];
	const branded = $derived(TAB_PAGES.includes(page.url.pathname));

	function toggleSheet(which: 'shelves' | 'more') {
		sheet = sheet === which ? null : which;
	}

	// The page keeps clear of the tabs (the --tabbar space in app.css), but
	// only while they are there: not on the login page, nor in the reader.
	$effect(() => {
		const html = document.documentElement;
		html.classList.toggle('has-tabbar', !!data.user);
		return () => html.classList.remove('has-tabbar');
	});

	// A sheet follows a finger that pulls it down by its grip, and goes when
	// let go far enough down.
	let pull = $state(0);
	let pullFrom: number | null = null;
	function grip(e: TouchEvent) {
		pullFrom = e.touches[0]?.clientY ?? null;
	}
	function gripMove(e: TouchEvent) {
		if (pullFrom === null) return;
		pull = Math.max(0, (e.touches[0]?.clientY ?? pullFrom) - pullFrom);
	}
	function gripEnd() {
		if (pull > 70) sheet = null;
		pull = 0;
		pullFrom = null;
	}

	// Once a new version has been published (SvelteKit polls for it), the next
	// navigation is a full page load, so the old code does not reach for
	// files that are no longer there. The banner lets the user do it at once.
	beforeNavigate(({ willUnload, to }) => {
		if (updated.current && !willUnload && to?.url) {
			location.href = to.url.href;
		}
	});

	// While a sheet is open the page behind it must not scroll.
	$effect(() => {
		document.body.style.overflow = sheet ? 'hidden' : '';
		return () => (document.body.style.overflow = '');
	});

	// Reading without the network (lib/offline): the service worker keeps the
	// app and the saved books at hand. Only in a built app: in development
	// it would stand between the editor and the page.
	let hasSaved = $state(false);

	// After an upgrade, a note that says so, with what is new a click away.
	// Per browser: the version last seen here. A browser seeing Legejo for
	// the first time has nothing to be told.
	const SEEN_KEY = 'legejo.version.seen';
	let upgradedFrom = $state<string | null>(null);
	let whatsNewOpen = $state(false);
	function noteVersion() {
		try {
			const seen = localStorage.getItem(SEEN_KEY);
			if (seen && seen !== __APP_VERSION__) upgradedFrom = seen;
			else if (!seen) localStorage.setItem(SEEN_KEY, __APP_VERSION__);
		} catch {
			// No storage: no note, and no harm.
		}
	}
	function versionSeen() {
		upgradedFrom = null;
		whatsNewOpen = false;
		try {
			localStorage.setItem(SEEN_KEY, __APP_VERSION__);
		} catch {
			// Shown again next time, then.
		}
	}
	onMount(() => {
		if (data.user) noteVersion();
		const online = () => {
			sendUnsent();
			sendUnsentTime();
			invalidateAll();
		};
		const recount = () => savedBooks().then((list) => (hasSaved = list.length > 0)).catch(() => {});
		window.addEventListener('online', online);
		window.addEventListener(CHANGED_EVENT, recount);
		if (data.user && availability() !== 'unsupported') {
			savedBooks().then((list) => (hasSaved = list.length > 0)).catch(() => {});
			if (!dev) {
				// A worker already in charge of the page kept what the layout just
				// fetched (service-worker/index.ts). Only one that takes over now,
				// on the first visit, needs to be handed it.
				const controlled = !!navigator.serviceWorker.controller;
				navigator.serviceWorker
					.register('/service-worker.js')
					.then(() => navigator.serviceWorker.ready)
					.then(() => (controlled ? undefined : warmSession()))
					.catch(() => {});
			}
			if (!data.offline) {
				sendUnsent();
				sendUnsentTime();
			}
		}
		return () => {
			window.removeEventListener('online', online);
			window.removeEventListener(CHANGED_EVENT, recount);
		};
	});
	afterNavigate(() => {
		if (data.user && availability() !== 'unsupported') savedBooks().then((list) => (hasSaved = list.length > 0)).catch(() => {});
		// After opening a followed shelf, its books are no longer new.
		if (data.user) refreshFollowNews();
	});

	async function logout() {
		await fetch('/api/auth/logout', { method: 'POST' });
		// What was kept on the device for reading without the network goes with the session.
		await forgetEverything().catch(() => {});
		await invalidateAll();
		goto('/login');
	}

	// "/" anywhere outside a text field jumps to the search page.
	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && sheet) {
			sheet = null;
			return;
		}
		if (e.key !== '/' || e.metaKey || e.ctrlKey || e.altKey) return;
		const el = e.target as HTMLElement | null;
		if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable)) return;
		e.preventDefault();
		goto('/search');
	}
</script>

<svelte:window {onkeydown} />

{#snippet search()}
	<a class="nav-item" href="/search">
		<Search size={15} strokeWidth={1.75} />
		{t('sidebar.search')}
	</a>
{/snippet}
{#snippet librarian()}
	{#if data.librarian}
		<a class="nav-item" href="/librarian">
			<Sparkles size={15} strokeWidth={1.75} />
			{t('librarian.heading')}
		</a>
	{/if}
{/snippet}
{#snippet library()}
	<a class="nav-item" href="/">
		<LibraryBig size={15} strokeWidth={1.75} />
		{t('sidebar.allBooks')}
	</a>
	<a class="nav-item" href="/reading">
		<BookOpenCheck size={15} strokeWidth={1.75} />
		{t('sidebar.reading')}
	</a>
{/snippet}
{#snippet mine()}
	<a class="nav-item" href="/notes">
		<Highlighter size={15} strokeWidth={1.75} />
		{t('sidebar.notes')}
	</a>
	{#if hasSaved || data.offline}
		<a class="nav-item" href="/offline">
			<HardDriveDownload size={15} strokeWidth={1.75} />
			{t('sidebar.offline')}
		</a>
	{/if}
	<a class="nav-item" href="/authors">
		<Feather size={15} strokeWidth={1.75} />
		{t('sidebar.authors')}
	</a>
{/snippet}
{#snippet shelves()}
	<h2>{t('sidebar.shelves')}</h2>
	{#if data.shelves.length === 0}
		<p class="none">{t('sidebar.noShelves')}</p>
	{:else}
		<ul class="shelf-list">
			{#each data.shelves as shelf (shelf.id)}
				<li>
					<a class="shelf-link" href={`/shelves/${shelf.id}`}>
						<BookMarked size={13} strokeWidth={1.75} class="shelf-icon" />
						<span class="shelf-name">{shelf.name}</span>
						{#if shelf.visibility === 'federated'}
							<span class="vis" title={t('sidebar.visFederated')} aria-label={t('sidebar.visFederated')}>
								<Earth size={12} strokeWidth={1.75} />
							</span>
						{:else if shelf.visibility === 'instance'}
							<span class="vis" title={t('sidebar.visInstance')} aria-label={t('sidebar.visInstance')}>
								<Globe size={12} strokeWidth={1.75} />
							</span>
						{:else if shelf.visibility === 'restricted'}
							<span class="vis" title={t('sidebar.visRestricted')} aria-label={t('sidebar.visRestricted')}>
								<Users size={12} strokeWidth={1.75} />
							</span>
						{/if}
						<span class="badge">{shelf.book_count}</span>
					</a>
				</li>
			{/each}
		</ul>
	{/if}
{/snippet}
{#snippet elsewhere()}
	<a class="nav-item lower" href="/public">
		<Globe size={15} strokeWidth={1.75} />
		{t('sidebar.public')}
		{#if newBookCount() > 0}
			<span class="badge new" title={t('follow.badge', { count: newBookCount() })} aria-label={t('follow.badge', { count: newBookCount() })}>
				{newBookCount()}
			</span>
		{/if}
	</a>
	{#if data.fed.available && data.fed.enabled}
		<a class="nav-item" href="/fediverse">
			<Earth size={15} strokeWidth={1.75} />
			{t('sidebar.fediverse')}
		</a>
	{/if}
	{#if data.catalogs}
		<a class="nav-item" href="/catalogs">
			<BookDown size={15} strokeWidth={1.75} />
			{t('sidebar.catalogs')}
		</a>
	{/if}
{/snippet}
{#snippet audiobooks()}
	{#if data.audiobooksEnabled}
		<a class="nav-item" href="/audiobooks">
			<Headphones size={15} strokeWidth={1.75} />
			{t('sidebar.audiobooks')}
		</a>
	{/if}
{/snippet}
{#snippet userRow(user: NonNullable<LayoutData['user']>)}
	<div class="user">
		<a class="account-link" href="/account" title={t('nav.account')}>
			<Avatar userId={user.id} hasAvatar={user.has_avatar} size={30} alt="" />
			<span class="name">{user.username}</span>
		</a>
		{#if user.is_admin}
			<a
				class="gear"
				href={data.fed.pending_instances ? '/admin?tab=federation' : '/admin'}
				title={data.fed.pending_instances ? t('nav.adminPending') : t('nav.admin')}
				aria-label={data.fed.pending_instances ? t('nav.adminPending') : t('nav.admin')}
			>
				<Settings size={15} />
				{#if data.fed.pending_instances}<span class="dot"></span>{/if}
			</a>
		{/if}
		<button class="logout" onclick={logout} title={t('nav.logout')} aria-label={t('nav.logout')}>
			<LogOut size={15} />
		</button>
	</div>
{/snippet}

{#if data.user}
	<div class="shell">
		<aside id="sidebar">
			<a class="sidebrand" href="/" aria-label={t('app.name')}>
				<Logo size={124} variant="pair" />
				<span class="wordmark">{t('app.name')} <span class="version">{__APP_VERSION__}</span></span>
			</a>

			<nav>
				{@render search()}
				{@render librarian()}
				{@render library()}
				{@render mine()}
				{@render shelves()}
				{@render elsewhere()}
				{@render audiobooks()}
			</nav>

			{@render userRow(data.user)}
		</aside>

		<main class:branded>
			{@render children()}
		</main>
	</div>

	{#if sheet}
		<div class="sheet-backdrop" onclick={() => (sheet = null)} aria-hidden="true"></div>
		<div
			class="sheet"
			role="dialog"
			aria-modal="true"
			aria-label={sheet === 'shelves' ? t('tabs.shelves') : t('tabs.more')}
			style:transform={pull > 0 ? `translateY(${pull}px)` : null}
		>
			<div class="grip" role="presentation" ontouchstart={grip} ontouchmove={gripMove} ontouchend={gripEnd} ontouchcancel={gripEnd}>
				<span></span>
				<button class="sheet-close" onclick={() => (sheet = null)} aria-label={t('nav.closeMenu')}>
					<X size={18} />
				</button>
			</div>
			{#if sheet === 'more'}
				<a class="sheetbrand" href="/" aria-label={t('app.name')}>
					<Logo size={64} variant="pair" />
					<span class="wordmark">{t('app.name')} <span class="version">{__APP_VERSION__}</span></span>
				</a>
			{/if}
			<nav>
				{#if sheet === 'shelves'}
					{@render shelves()}
					{@render elsewhere()}
				{:else}
					{@render mine()}
					{@render audiobooks()}
					{@render librarian()}
				{/if}
			</nav>
			{#if sheet === 'more'}{@render userRow(data.user)}{/if}
		</div>
	{/if}

	<nav class="tabbar" aria-label={t('tabs.label')}>
		<a href="/" class:on={tab === 'library'} aria-current={tab === 'library' ? 'page' : undefined}>
			<LibraryBig size={22} strokeWidth={1.75} />
			<span>{t('tabs.library')}</span>
		</a>
		<a href="/reading" class:on={tab === 'reading'} aria-current={tab === 'reading' ? 'page' : undefined}>
			<BookOpenCheck size={22} strokeWidth={1.75} />
			<span>{t('tabs.reading')}</span>
		</a>
		<a href="/search" class:on={tab === 'search'} aria-current={tab === 'search' ? 'page' : undefined}>
			<Search size={22} strokeWidth={1.75} />
			<span>{t('sidebar.search')}</span>
		</a>
		<button class:on={tab === 'shelves'} aria-expanded={sheet === 'shelves'} onclick={() => toggleSheet('shelves')}>
			<span class="icon">
				<BookMarked size={22} strokeWidth={1.75} />
				{#if newBookCount() > 0}<span class="dot" aria-label={t('follow.badge', { count: newBookCount() })}></span>{/if}
			</span>
			<span>{t('tabs.shelves')}</span>
		</button>
		<button class:on={tab === 'more'} aria-expanded={sheet === 'more'} onclick={() => toggleSheet('more')}>
			<span class="icon">
				<Ellipsis size={22} strokeWidth={1.75} />
				{#if data.user.is_admin && data.fed.pending_instances}<span class="dot" aria-label={t('nav.adminPending')}></span>{/if}
			</span>
			<span>{t('tabs.more')}</span>
		</button>
	</nav>
{:else}
	{@render children()}
{/if}

{#if updated.current}
	<div class="update" role="status">
		<span>{t('update.available')}</span>
		<button type="button" onclick={() => location.reload()}>{t('update.reload')}</button>
	</div>
{:else if upgradedFrom && data.user}
	<div class="update" role="status">
		<span>{t('whatsnew.updated', { version: __APP_VERSION__ })}</span>
		<button type="button" onclick={() => (whatsNewOpen = true)}>{t('whatsnew.show')}</button>
		<button type="button" class="ghost" onclick={versionSeen}>{t('whatsnew.dismiss')}</button>
	</div>
{/if}
{#if whatsNewOpen}
	<WhatsNew since={upgradedFrom} onclose={versionSeen} />
{/if}

<style>
	/* "A new version is ready": a small note above everything, in the way of nothing. */
	.update {
		/* Centred by auto margins: a fixed box at left: 50% would only get half
		   the viewport to lay itself out in, and wrap its text on a phone. */
		position: fixed;
		left: 0;
		right: 0;
		bottom: calc(1rem + var(--tabbar, env(safe-area-inset-bottom)));
		margin: 0 auto;
		width: fit-content;
		z-index: 70;
		display: flex;
		align-items: center;
		gap: 0.75rem;
		max-width: calc(100vw - 2rem);
		padding: 0.6rem 0.6rem 0.6rem 1rem;
		border-radius: 8px;
		background: var(--fg);
		color: var(--bg);
		font-size: 0.9rem;
		box-shadow: 0 6px 24px rgb(0 0 0 / 0.3);
	}
	.update button {
		flex-shrink: 0;
		background: var(--bg);
		color: var(--fg);
		font-size: 0.85rem;
		padding: 0.35rem 0.7rem;
	}

	.shell {
		display: grid;
		grid-template-columns: 14rem minmax(0, 1fr);
		gap: 2.5rem;
		max-width: 72rem;
		margin: 0 auto;
		padding: 1.5rem 1rem;
		align-items: start;
	}
	main {
		min-width: 0;
	}
	/* The sidebar never grows past the window: the navigation scrolls inside
	   it, so the user row at the bottom stays reachable however many shelves
	   there are (a sticky element taller than the viewport can't be scrolled). */
	aside {
		position: sticky;
		top: 1.5rem;
		max-height: calc(100vh - 3rem);
		max-height: calc(100dvh - 3rem);
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}
	.sidebrand,
	.user {
		flex-shrink: 0;
	}
	.sidebrand {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.35rem;
		text-decoration: none;
		padding-bottom: 1rem;
		border-bottom: 3px double var(--border);
		color: var(--fg);
	}
	.wordmark {
		font-size: 0.72rem;
		letter-spacing: 0.04em;
		color: var(--muted);
	}
	.version {
		font-variant-numeric: tabular-nums;
	}
	.sidebrand:hover {
		color: var(--accent);
	}
	.user {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		margin-top: 0.5rem;
		padding-top: 1rem;
		border-top: 1px solid var(--border);
	}
	.account-link {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 0.6rem;
		color: var(--fg);
		border-radius: 6px;
		padding: 0.2rem 0.4rem;
		margin: -0.2rem -0.4rem;
	}
	.account-link:hover {
		text-decoration: none;
		background: var(--card);
	}
	.name {
		font-size: 0.9rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex: 1;
	}
	.gear {
		position: relative;
		padding: 0.3rem;
		color: var(--muted);
		display: flex;
		border-radius: 6px;
	}
	.gear .dot {
		position: absolute;
		top: 0.15rem;
		right: 0.15rem;
		width: 0.45rem;
		height: 0.45rem;
		border-radius: 50%;
		background: var(--danger);
	}
	.gear:hover {
		color: var(--fg);
		background: var(--card);
	}
	.logout {
		background: none;
		border: none;
		padding: 0.3rem;
		color: var(--muted);
		display: flex;
		border-radius: 6px;
	}
	.logout:hover {
		color: var(--danger);
		background: var(--card);
	}
	nav {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
		overscroll-behavior: contain;
		/* Room for the hover backgrounds that reach 0.5rem outside. */
		padding: 0 0.5rem;
		margin: 0 -0.5rem;
		scrollbar-width: thin;
	}
	.nav-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		color: var(--fg);
		font-size: 0.9rem;
		font-weight: 600;
		padding: 0.35rem 0.5rem;
		border-radius: 6px;
		margin: 0 -0.5rem 0.5rem;
	}
	.nav-item.lower {
		margin-top: 0.9rem;
	}
	.nav-item:hover {
		background: var(--card);
		text-decoration: none;
	}
	h2 {
		font-size: 0.72rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--muted);
		margin: 0 0 0.35rem;
	}
	.none {
		font-size: 0.85rem;
		color: var(--muted);
		font-style: italic;
		margin: 0;
	}
	.shelf-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
	}
	.shelf-link {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		padding: 0.3rem 0.5rem;
		margin: 0 -0.5rem;
		border-radius: 6px;
		font-size: 0.9rem;
		color: var(--fg);
	}
	.shelf-link:hover {
		background: var(--card);
		text-decoration: none;
	}
	.shelf-link :global(.shelf-icon) {
		color: var(--gold);
		flex-shrink: 0;
	}
	.shelf-name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* Public/federated marker: quiet, so a long list stays calm. */
	.vis {
		display: inline-flex;
		flex-shrink: 0;
		color: var(--muted);
		opacity: 0.75;
	}
	.badge {
		font-size: 0.72rem;
		color: var(--muted);
		background: var(--card);
		border: 1px solid var(--border);
		border-radius: 99px;
		padding: 0.05rem 0.5rem;
		flex-shrink: 0;
	}
	/* Books not yet seen on followed shelves. */
	.badge.new {
		margin-left: auto;
		color: var(--bg);
		background: var(--accent);
		border-color: var(--accent);
	}
	/* Mobile only: the tabs at the foot of the screen and the sheets they open. */
	.tabbar {
		display: none;
	}
	@media (max-width: 48rem) {
		/* No bar at the top: the pages have headings of their own, and the
		   pigeon and the version are in the More sheet. */
		.shell {
			grid-template-columns: 1fr;
			gap: 1.5rem;
			padding-top: calc(1.25rem + env(safe-area-inset-top));
			padding-bottom: calc(1.5rem + var(--tabbar, 0px));
		}
		aside {
			display: none;
		}
		/* Without the sidebar's pigeons, the heading of a page the tabs lead
		   to has one beside it. */
		main.branded :global(h1)::before {
			content: '';
			display: inline-block;
			width: 0.76em;
			height: 1.15em;
			margin-right: 0.4em;
			vertical-align: -0.2em;
			background: url('../../lib/assets/duva.svg') center / contain no-repeat;
		}

		.tabbar {
			position: fixed;
			left: 0;
			right: 0;
			bottom: 0;
			z-index: 47;
			display: grid;
			grid-template-columns: repeat(5, 1fr);
			padding: 0.3rem calc(0.25rem + env(safe-area-inset-right)) calc(0.3rem + env(safe-area-inset-bottom))
				calc(0.25rem + env(safe-area-inset-left));
			background: color-mix(in srgb, var(--bg) 92%, transparent);
			-webkit-backdrop-filter: blur(14px);
			backdrop-filter: blur(14px);
			border-top: 1px solid var(--border);
		}
		.tabbar > a,
		.tabbar > button {
			display: flex;
			flex-direction: column;
			align-items: center;
			justify-content: center;
			gap: 0.15rem;
			min-height: 2.8rem;
			padding: 0.15rem 0;
			background: none;
			border: none;
			border-radius: 10px;
			color: var(--muted);
			font: inherit;
			font-size: 0.66rem;
			font-weight: 500;
			letter-spacing: 0.01em;
			text-decoration: none;
			-webkit-tap-highlight-color: transparent;
		}
		.tabbar > .on {
			color: var(--accent);
		}
		.tabbar .icon {
			position: relative;
			display: flex;
		}
		.tabbar .dot {
			position: absolute;
			top: -0.1rem;
			right: -0.25rem;
			width: 0.5rem;
			height: 0.5rem;
			border-radius: 50%;
			background: var(--accent);
			border: 2px solid var(--bg);
		}

		/* A sheet comes up above the tabs, which stay in reach to close it or
		   switch to the other. */
		.sheet-backdrop {
			position: fixed;
			inset: 0;
			z-index: 45;
			background: rgb(0 0 0 / 0.3);
			animation: fade 0.2s ease;
		}
		.sheet {
			position: fixed;
			left: 0;
			right: 0;
			bottom: var(--tabbar, 0px);
			z-index: 46;
			display: flex;
			flex-direction: column;
			gap: 0.75rem;
			max-height: calc(100dvh - var(--tabbar, 0px) - 4rem - env(safe-area-inset-top));
			padding: 0 calc(1.25rem + env(safe-area-inset-right)) 1rem calc(1.25rem + env(safe-area-inset-left));
			background: var(--bg);
			border-radius: 16px 16px 0 0;
			box-shadow: 0 -6px 28px rgb(0 0 0 / 0.18);
			animation: rise 0.22s cubic-bezier(0.2, 0.8, 0.2, 1);
		}
		.grip {
			position: relative;
			flex-shrink: 0;
			display: flex;
			justify-content: center;
			padding: 0.55rem 0 0.25rem;
			touch-action: none;
		}
		.grip > span {
			width: 2.4rem;
			height: 0.3rem;
			border-radius: 99px;
			background: var(--border);
		}
		.sheet-close {
			position: absolute;
			right: -0.6rem;
			top: 0.3rem;
			display: inline-flex;
			padding: 0.35rem;
			background: none;
			border: none;
			border-radius: 6px;
			color: var(--muted);
		}
		.sheet nav {
			padding-top: 0.25rem;
		}
		.sheet .nav-item,
		.sheet .shelf-link {
			padding-top: 0.55rem;
			padding-bottom: 0.55rem;
			font-size: 1rem;
		}
		.sheet .user {
			margin-top: 0;
		}
		.sheetbrand {
			display: flex;
			flex-direction: column;
			align-items: center;
			gap: 0.25rem;
			padding: 0 0 0.75rem;
			border-bottom: 3px double var(--border);
			color: var(--fg);
			text-decoration: none;
		}
	}
	@keyframes rise {
		from {
			transform: translateY(100%);
		}
	}
	@keyframes fade {
		from {
			opacity: 0;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.sheet,
		.sheet-backdrop {
			animation: none;
		}
	}
</style>
