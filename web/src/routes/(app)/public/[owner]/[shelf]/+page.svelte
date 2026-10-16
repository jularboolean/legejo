<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { untrack } from 'svelte';
	import { ArrowLeft, Bell, BellOff, Check, Download, Rss } from '@lucide/svelte';
	import { refreshFollowNews } from '#lib/follows.svelte';
	import Avatar from '#lib/Avatar.svelte';
	import { getLocale, t } from '#lib/i18n';
	import { renderMarkdown } from '#lib/markdown';
	import { defaultShelfCover } from '#lib/shelfCovers';
	import {
		SHARED_SORT_KEYS,
		loadSharedSort,
		loadView,
		saveSharedSort,
		saveView,
		sortShared,
		type SortDir,
		type SortKey,
		type ViewMode
	} from '#lib/library';
	import BookRows from '#lib/library/BookRows.svelte';
	import SortControl from '#lib/library/SortControl.svelte';
	import { cover } from '#lib/cover';
	import ViewSwitch from '#lib/library/ViewSwitch.svelte';
	import type { PublicBook } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const shelf = $derived(data.publicShelf);
	const shelfPath = $derived(
		`/public/${encodeURIComponent(shelf.owner)}/${encodeURIComponent(shelf.name)}`
	);
	const descriptionHtml = $derived(shelf.description ? renderMarkdown(shelf.description) : '');

	const bookPath = (book: PublicBook) => `${shelfPath}/${book.uuid}`;
	const bookCover = (book: PublicBook) =>
		`/api/public/books/${book.id}/cover?size=thumb&v=${encodeURIComponent(book.updated_at ?? book.created_at)}`;

	// Shared with the library, the own shelves and the search page.
	let view = $state<ViewMode>(loadView());

	function setView(next: ViewMode) {
		view = next;
		saveView(next);
	}

	// One order for every shared shelf, remembered on this device.
	let order = $state(loadSharedSort());
	const books = $derived(sortShared(shelf.books, order.sort, order.dir, getLocale()));

	function setOrder(sort: SortKey, dir: SortDir) {
		order = { sort, dir };
		saveSharedSort(sort, dir);
	}

	// Following. Opening the page marks the shelf as seen on the server, so
	// a reload (after an import, say) says nothing is new any more: what
	// was new is decided once, from the first load of this shelf.
	const initial = untrack(() => data.publicShelf);
	let follow = $state(initial.follow);
	let feedOpen = $state(false);
	let followBusy = $state(false);
	let firstLook = $state<{ shelf: number; seenAt: string | null }>({
		shelf: initial.id,
		seenAt: initial.follow.followed ? initial.follow.seen_at : null
	});
	$effect.pre(() => {
		if (firstLook.shelf !== shelf.id) {
			firstLook = { shelf: shelf.id, seenAt: shelf.follow.followed ? shelf.follow.seen_at : null };
			follow = shelf.follow;
			feedOpen = false;
		}
	});
	const isNew = (book: PublicBook) =>
		!!firstLook.seenAt && !!book.added_to_shelf && book.added_to_shelf > firstLook.seenAt;

	async function toggleFollow() {
		followBusy = true;
		errorMsg = '';
		try {
			const res = await fetch(`/api/public/shelves/${shelf.id}/follow`, {
				method: follow.followed ? 'DELETE' : 'PUT'
			});
			if (!res.ok) {
				errorMsg = t('edit.saveFailed');
			} else if (follow.followed) {
				follow = { followed: false, new_books: 0, seen_at: null, feed: null };
				feedOpen = false;
			} else {
				follow = await res.json();
			}
			refreshFollowNews();
		} catch {
			errorMsg = t('common.network');
		} finally {
			followBusy = false;
		}
	}

	let importing = $state<Set<number>>(new Set());
	let imported = $state<Set<number>>(new Set());
	let errorMsg = $state('');

	async function importBook(id: number) {
		importing = new Set([...importing, id]);
		errorMsg = '';
		try {
			const res = await fetch(`/api/public/books/${id}/import`, { method: 'POST' });
			if (res.ok) {
				imported = new Set([...imported, id]);
				await invalidateAll();
			} else {
				const body = await res.json().catch(() => null);
				errorMsg = body?.error ?? t('public.importFailed');
			}
		} catch {
			errorMsg = t('common.network');
		} finally {
			const next = new Set(importing);
			next.delete(id);
			importing = next;
		}
	}
</script>

<a class="back" href="/public"><ArrowLeft size={13} /> {t('public.back')}</a>

<header class="hero has-image">
	<img
		class="hero-image"
		src={shelf.has_cover ? `/api/public/shelves/${shelf.id}/cover` : defaultShelfCover(shelf.id)}
		alt=""
	/>
	<div class="hero-shade"></div>
	<div class="hero-content">
		<h1>{shelf.name}</h1>
		<p class="meta">
			<Avatar userId={shelf.owner_id} hasAvatar={shelf.owner_has_avatar} size={20} alt="" />
			{t('public.by', { owner: shelf.owner })} · {shelf.book_count}
			{#if shelf.restricted}· {t('public.restricted')}{/if}
		</p>
	</div>
</header>

{#if descriptionHtml}
	<div class="description">{@html descriptionHtml}</div>
{/if}

<div class="follow">
	{#if follow.followed}
		<span class="following"><Check size={13} /> {t('follow.following')}</span>
		{#if follow.feed}
			<button type="button" class="link" onclick={() => (feedOpen = !feedOpen)} aria-expanded={feedOpen}>
				<Rss size={13} /> {t('follow.feed')}
			</button>
		{/if}
		<button type="button" class="link quiet" disabled={followBusy} onclick={toggleFollow}>
			<BellOff size={13} /> {t('follow.unfollow')}
		</button>
	{:else}
		<button type="button" disabled={followBusy} onclick={toggleFollow}>
			<Bell size={13} /> {t('follow.follow')}
		</button>
		<span class="hint">{t('follow.hint')}</span>
	{/if}
</div>
{#if feedOpen && follow.feed}
	<div class="feed">
		<input readonly value={follow.feed} onfocus={(e) => e.currentTarget.select()} aria-label={t('follow.feed')} />
		<p class="hint">{t('follow.feedHint')}</p>
	</div>
{/if}

<div class="bar">
	<p class="hint">{t('public.importHint')}</p>
	{#if shelf.books.length > 0}
		<div class="controls">
			{#if shelf.books.length > 1}
				<SortControl keys={SHARED_SORT_KEYS} sort={order.sort} dir={order.dir} onchange={setOrder} />
			{/if}
			<ViewSwitch {view} onchange={setView} />
		</div>
	{/if}
</div>
{#if errorMsg}<p class="error">{errorMsg}</p>{/if}

{#snippet importAction(book: PublicBook)}
	{#if isNew(book)}<span class="new">{t('follow.new')}</span>{/if}
	{#if book.owned}
		<span class="done"><Check size={13} /> {t('public.owned')}</span>
	{:else if imported.has(book.id)}
		<span class="done"><Check size={13} /> {t('public.imported')}</span>
	{:else}
		<button class="ghost import" disabled={importing.has(book.id)} onclick={() => importBook(book.id)}>
			<Download size={13} />
			{importing.has(book.id) ? t('public.importing') : t('public.import')}
		</button>
	{/if}
{/snippet}

{#if view === 'list'}
	<BookRows {books} href={bookPath} cover={bookCover} own={false} action={importAction} />
{:else}
	<div class="grid">
		{#each books as book (book.id)}
			<div class="card">
				<a class="cover turning" style:--turn="5" href={bookPath(book)}>
					{#if book.has_cover}
						<img use:cover src={bookCover(book)} alt={book.title} loading="lazy" />
					{:else}
						<span class="placeholder">{book.title}</span>
					{/if}
				</a>
				<div class="meta-col">
					<a class="title" href={bookPath(book)} title={book.title}>{book.title}</a>
					{#if book.author}<span class="author">{book.author}</span>{/if}
				</div>
				{@render importAction(book)}
			</div>
		{/each}
	</div>
{/if}

<style>
	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		margin-bottom: 1.25rem;
		color: var(--muted);
	}
	.hero {
		position: relative;
		border-radius: 12px;
		overflow: hidden;
		margin-bottom: 1.25rem;
	}
	.hero.has-image {
		min-height: 9rem;
		display: flex;
		align-items: flex-end;
	}
	.hero-image {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.hero-shade {
		position: absolute;
		inset: 0;
		background: linear-gradient(to top, rgba(0, 0, 0, 0.65) 0%, rgba(0, 0, 0, 0.1) 55%, transparent);
	}
	.hero-content {
		position: relative;
		width: 100%;
	}
	.has-image .hero-content {
		padding: 0.9rem 1.25rem;
		color: #fff;
	}
	.has-image .meta {
		color: rgba(255, 255, 255, 0.85);
	}
	h1 {
		margin: 0;
		font-size: 1.5rem;
	}
	.has-image h1 {
		font-size: 1.8rem;
		text-shadow: 0 1px 4px rgba(0, 0, 0, 0.4);
	}
	.meta {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		margin: 0.35rem 0 0;
		color: var(--muted);
		font-size: 0.9rem;
	}
	.description {
		margin: 0.25rem 0 1.75rem;
		max-width: 42rem;
		padding-left: 1rem;
		border-left: 3px solid var(--gold);
		color: var(--muted);
		font-size: 0.95rem;
		line-height: 1.6;
	}
	.description :global(p) {
		margin: 0.5rem 0;
	}
	.description :global(p:first-child) {
		margin-top: 0;
	}
	.description :global(p:last-child) {
		margin-bottom: 0;
	}
	.description :global(a) {
		color: var(--accent);
	}
	.bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		margin: 0 0 1.25rem;
	}
	.controls {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-shrink: 0;
	}
	@media (max-width: 560px) {
		.bar {
			flex-direction: column;
			align-items: stretch;
			gap: 0.6rem;
		}
		.controls {
			justify-content: space-between;
		}
	}
	.hint {
		margin: 0;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr));
		gap: 1.25rem;
	}
	.card {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.cover {
		aspect-ratio: 2 / 3;
		border-radius: 3px 6px 6px 3px;
		overflow: hidden;
		background: var(--card);
		border: 1px solid var(--border);
		display: flex;
		box-shadow: var(--shadow);
	}
	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.placeholder {
		margin: auto;
		padding: 0.75rem;
		text-align: center;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.meta-col {
		display: flex;
		flex-direction: column;
	}
	.title {
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--fg);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.title:hover {
		color: var(--accent);
		text-decoration: none;
	}
	.author {
		font-size: 0.8rem;
		color: var(--muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.import {
		font-size: 0.78rem;
		padding: 0.3rem 0.6rem;
		justify-content: center;
		white-space: nowrap;
	}
	.follow {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem 0.9rem;
		margin: 0 0 1rem;
	}
	.follow > button:first-child {
		font-size: 0.85rem;
	}
	.following {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.85rem;
		color: var(--accent);
	}
	.follow .link {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		background: none;
		border: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.follow .link.quiet {
		color: var(--muted);
	}
	.follow .link:hover {
		text-decoration: underline;
		filter: none;
	}
	.feed {
		margin: -0.4rem 0 1.25rem;
		max-width: 36rem;
	}
	.feed input {
		width: 100%;
		font-size: 0.78rem;
	}
	.feed .hint {
		margin-top: 0.3rem;
	}
	.new {
		align-self: flex-start;
		font-size: 0.7rem;
		font-weight: 600;
		color: var(--bg);
		background: var(--accent);
		border-radius: 99px;
		padding: 0 0.45rem;
		white-space: nowrap;
	}
	.done {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.3rem;
		font-size: 0.8rem;
		color: var(--accent);
		padding: 0.3rem 0;
		white-space: nowrap;
	}
</style>
