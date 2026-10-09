<script lang="ts">
	import { afterNavigate, goto, invalidateAll } from '$app/navigation';
	import {
		BookDown,
		BookMarked,
		BookOpenCheck,
		Earth,
		Feather,
		Globe,
		Headphones,
		LibraryBig,
		LogOut,
		Menu,
		Search,
		Settings,
		Sparkles,
		Users,
		X
	} from '@lucide/svelte';
	import Avatar from '#lib/Avatar.svelte';
	import Logo from '#lib/Logo.svelte';
	import { t } from '#lib/i18n';
	import type { LayoutData } from './$types';

	let { data, children }: { data: LayoutData; children: any } = $props();

	let menuOpen = $state(false);
	afterNavigate(() => (menuOpen = false));

	// While the mobile drawer is open the page behind it must not scroll.
	$effect(() => {
		document.body.style.overflow = menuOpen ? 'hidden' : '';
		return () => (document.body.style.overflow = '');
	});

	async function logout() {
		await fetch('/api/auth/logout', { method: 'POST' });
		await invalidateAll();
		goto('/login');
	}

	// "/" anywhere outside a text field jumps to the search page.
	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && menuOpen) {
			menuOpen = false;
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

{#if data.user}
	<header class="topbar">
		<div class="topbar-inner">
			<button
				class="menu-toggle"
				onclick={() => (menuOpen = !menuOpen)}
				aria-expanded={menuOpen}
				aria-controls="sidebar"
				aria-label={t('nav.menu')}
			>
				<Menu size={20} />
			</button>
			<span class="wordmark">{t('app.name')} <span class="version">{__APP_VERSION__}</span></span>
			<a class="brand" href="/" aria-label={t('app.name')}>
				<Logo size={40} />
			</a>
		</div>
	</header>

	<div class="shell">
		<!-- Mobile: tapping outside the drawer closes it. -->
		<div
			class="backdrop"
			class:open={menuOpen}
			onclick={() => (menuOpen = false)}
			aria-hidden="true"
		></div>
		<aside id="sidebar" class:open={menuOpen}>
			<div class="drawer-head">
				<button class="menu-close" onclick={() => (menuOpen = false)} aria-label={t('nav.closeMenu')}>
					<X size={20} />
				</button>
			</div>
			<a class="sidebrand" href="/" aria-label={t('app.name')}>
				<Logo size={124} variant="pair" />
				<span class="wordmark">{t('app.name')} <span class="version">{__APP_VERSION__}</span></span>
			</a>

			<nav>
				<a class="nav-item" href="/search">
					<Search size={15} strokeWidth={1.75} />
					{t('sidebar.search')}
				</a>
				{#if data.librarian}
					<a class="nav-item" href="/librarian">
						<Sparkles size={15} strokeWidth={1.75} />
						{t('librarian.heading')}
					</a>
				{/if}
				<a class="nav-item" href="/">
					<LibraryBig size={15} strokeWidth={1.75} />
					{t('sidebar.allBooks')}
				</a>
				<a class="nav-item" href="/reading">
					<BookOpenCheck size={15} strokeWidth={1.75} />
					{t('sidebar.reading')}
				</a>
				<a class="nav-item" href="/authors">
					<Feather size={15} strokeWidth={1.75} />
					{t('sidebar.authors')}
				</a>

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

				<a class="nav-item lower" href="/public">
					<Globe size={15} strokeWidth={1.75} />
					{t('sidebar.public')}
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
				{#if data.audiobooksEnabled}
					<a class="nav-item" href="/audiobooks">
						<Headphones size={15} strokeWidth={1.75} />
						{t('sidebar.audiobooks')}
					</a>
				{/if}
			</nav>

			<div class="user">
				<a class="account-link" href="/account" title={t('nav.account')}>
					<Avatar userId={data.user.id} hasAvatar={data.user.has_avatar} size={30} alt="" />
					<span class="name">{data.user.username}</span>
				</a>
				{#if data.user.is_admin}
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
		</aside>

		<main>
			{@render children()}
		</main>
	</div>
{:else}
	{@render children()}
{/if}

<style>
	/* Mobile only: on desktop the sidebar carries the logo and navigation. */
	.topbar {
		display: none;
		position: sticky;
		top: 0;
		z-index: 40;
		background: var(--bg);
		border-bottom: 1px solid var(--border);
	}
	.topbar-inner {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		max-width: 72rem;
		margin: 0 auto;
		padding: 0.5rem 1rem;
	}
	.topbar .wordmark {
		flex: 1;
		text-align: center;
	}
	.brand {
		flex-shrink: 0;
		display: flex;
	}
	.brand :global(.logo) {
		display: block;
	}
	.menu-toggle {
		display: none;
		background: none;
		border: none;
		padding: 0.3rem;
		color: var(--muted);
		border-radius: 6px;
	}
	.menu-close {
		background: none;
		border: none;
		padding: 0.3rem;
		color: var(--muted);
		border-radius: 6px;
		display: inline-flex;
	}
	.menu-toggle:hover,
	.menu-close:hover {
		color: var(--fg);
		background: var(--card);
		filter: none;
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
	.backdrop,
	.drawer-head {
		display: none;
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
	@media (max-width: 48rem) {
		.topbar {
			display: block;
		}
		.menu-toggle {
			display: inline-flex;
		}
		.shell {
			grid-template-columns: 1fr;
			gap: 1.5rem;
		}
		/* The sidebar becomes a drawer that slides in from the left. */
		aside {
			position: fixed;
			top: 0;
			left: 0;
			bottom: 0;
			z-index: 60;
			width: min(18rem, 85vw);
			max-height: none;
			padding: 0.75rem 1.25rem calc(1rem + env(safe-area-inset-bottom));
			gap: 0.75rem;
			background: var(--bg);
			border-right: 1px solid var(--border);
			box-shadow: 0 0 2rem rgb(0 0 0 / 0.18);
			transform: translateX(-105%);
			visibility: hidden;
			transition:
				transform 0.22s ease,
				visibility 0s linear 0.22s;
		}
		aside.open {
			transform: none;
			visibility: visible;
			transition: transform 0.22s ease;
		}
		.drawer-head {
			display: flex;
			flex-shrink: 0;
			margin: 0 -0.3rem;
		}
		.backdrop {
			display: block;
			position: fixed;
			inset: 0;
			z-index: 50;
			background: rgb(0 0 0 / 0.35);
			opacity: 0;
			pointer-events: none;
			transition: opacity 0.22s ease;
		}
		.backdrop.open {
			opacity: 1;
			pointer-events: auto;
		}
		.sidebrand {
			display: none;
		}
		.user {
			padding-top: 0.75rem;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		aside,
		aside.open,
		.backdrop {
			transition: none;
		}
	}
</style>
