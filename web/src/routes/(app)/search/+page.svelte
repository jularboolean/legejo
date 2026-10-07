<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { Check, Download, Headphones, Search } from '@lucide/svelte';
	import { formatLength } from '#lib/audio';
	import Avatar from '#lib/Avatar.svelte';
	import { defaultShelfCover } from '#lib/shelfCovers';
	import { t } from '#lib/i18n';
	import { loadView, saveView, type ViewMode } from '#lib/library';
	import BookGrid from '#lib/library/BookGrid.svelte';
	import BookRows from '#lib/library/BookRows.svelte';
	import ViewSwitch from '#lib/library/ViewSwitch.svelte';
	import type { PublicHit } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	let query = $state(data.q);
	let debounce: ReturnType<typeof setTimeout>;
	let input = $state<HTMLInputElement>();

	$effect(() => {
		input?.focus();
	});

	function onInput() {
		clearTimeout(debounce);
		debounce = setTimeout(() => {
			const target = query.trim()
				? `/search?q=${encodeURIComponent(query.trim())}`
				: '/search';
			// reset: false keeps the caret in the box (and the scroll position)
			// while the results change under it.
			goto(target, { replace: true, reset: false });
		}, 250);
	}

	const result = $derived(data.result);
	const nothing = $derived(
		data.q !== '' &&
			result.mine.length === 0 &&
			result.public.length === 0 &&
			result.shelves.length === 0 &&
			result.audiobooks.length === 0
	);

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

	function publicBookPath(hit: PublicHit): string {
		return `/public/${encodeURIComponent(hit.owner)}/${encodeURIComponent(hit.shelf_name)}/${hit.uuid}`;
	}

	function publicCover(hit: PublicHit): string {
		return `/api/public/books/${hit.id}/cover?size=thumb&v=${encodeURIComponent(hit.updated_at ?? hit.created_at)}`;
	}

	// Shared with the library and the shelf pages.
	let view = $state<ViewMode>(loadView());

	function setView(next: ViewMode) {
		view = next;
		saveView(next);
	}
</script>

{#snippet hitContext(hit: PublicHit)}
	{t('search.inShelf', { shelf: hit.shelf_name, by: t('public.by', { owner: hit.owner }) })}
{/snippet}

{#snippet hitAction(hit: PublicHit)}
	{#if hit.owned}
		<span class="done"><Check size={13} /> {t('public.owned')}</span>
	{:else if imported.has(hit.id)}
		<span class="done"><Check size={13} /> {t('public.imported')}</span>
	{:else}
		<button class="ghost import" disabled={importing.has(hit.id)} onclick={() => importBook(hit.id)}>
			<Download size={13} />
			{importing.has(hit.id) ? t('public.importing') : t('public.import')}
		</button>
	{/if}
{/snippet}

<h1>{t('search.heading')}</h1>

<div class="top">
	<div class="searchbox">
		<Search size={15} class="search-icon" aria-hidden="true" />
		<input
			type="search"
			placeholder={t('search.placeholder')}
			bind:value={query}
			bind:this={input}
			oninput={onInput}
		/>
	</div>
	{#if result.mine.length > 0 || result.public.length > 0}
		<ViewSwitch {view} onchange={setView} />
	{/if}
</div>

{#if errorMsg}<p class="error">{errorMsg}</p>{/if}

{#if nothing}
	<p class="empty">{t('search.none')}</p>
{/if}

{#if result.mine.length > 0}
	<section>
		<h2>{t('search.mine')}</h2>
		{#if view === 'list'}
			<BookRows books={result.mine} />
		{:else}
			<BookGrid books={result.mine} />
		{/if}
	</section>
{/if}

{#if result.audiobooks.length > 0}
	<section>
		<h2>{t('sidebar.audiobooks')}</h2>
		<div class="shelfgrid">
			{#each result.audiobooks as book (book.id)}
				<a class="shelfcard" href={`/audiobooks/${book.id}`}>
					<div class="shelfcover">
						{#if book.has_cover}
							<img
								src={`/api/audiobooks/${book.id}/cover?v=${encodeURIComponent(book.updated_at ?? '')}`}
								alt=""
								loading="lazy"
							/>
						{:else}
							<Headphones size={20} strokeWidth={1.5} />
						{/if}
					</div>
					<div class="shelfmeta">
						<span class="name">{book.title || t('audio.untitled')}</span>
						<span class="sub">{[book.author, formatLength(book.seconds)].filter(Boolean).join(' · ')}</span>
					</div>
				</a>
			{/each}
		</div>
	</section>
{/if}

{#if result.public.length > 0}
	<section>
		<h2>{t('search.public')}</h2>
		{#if view === 'list'}
			<BookRows
				books={result.public}
				href={publicBookPath}
				cover={publicCover}
				own={false}
				context={hitContext}
				action={hitAction}
			/>
		{:else}
			<div class="grid">
				{#each result.public as hit (hit.id)}
					<div class="card">
						<a class="cover" href={publicBookPath(hit)}>
							{#if hit.has_cover}
								<img src={publicCover(hit)} alt={hit.title} loading="lazy" />
							{:else}
								<span class="placeholder">{hit.title}</span>
							{/if}
						</a>
						<a class="title" href={publicBookPath(hit)} title={hit.title}>{hit.title}</a>
						{#if hit.author}<span class="sub">{hit.author}</span>{/if}
						<span class="context">{@render hitContext(hit)}</span>
						{@render hitAction(hit)}
					</div>
				{/each}
			</div>
		{/if}
	</section>
{/if}

{#if result.shelves.length > 0}
	<section>
		<h2>{t('search.publicShelves')}</h2>
		<div class="shelfgrid">
			{#each result.shelves as shelf (shelf.id)}
				<a
					class="shelfcard"
					href={`/public/${encodeURIComponent(shelf.owner)}/${encodeURIComponent(shelf.name)}`}
				>
					<div class="shelfcover">
						<img
							src={shelf.has_cover ? `/api/public/shelves/${shelf.id}/cover` : defaultShelfCover(shelf.id)}
							alt=""
							loading="lazy"
						/>
					</div>
					<div class="shelfmeta">
						<span class="name">{shelf.name}</span>
						<span class="sub subrow">
							<Avatar userId={shelf.owner_id} hasAvatar={shelf.owner_has_avatar} size={16} alt="" />
							{t('public.by', { owner: shelf.owner })} · {shelf.book_count}
						</span>
					</div>
				</a>
			{/each}
		</div>
	</section>
{/if}

<style>
	h1 {
		font-size: 1.4rem;
		margin: 0 0 1rem;
	}
	.top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		margin-bottom: 1.75rem;
	}
	.searchbox {
		position: relative;
		flex: 1;
		max-width: 28rem;
	}
	.searchbox :global(.search-icon) {
		position: absolute;
		left: 0.7rem;
		top: 50%;
		transform: translateY(-50%);
		color: var(--muted);
		pointer-events: none;
	}
	.searchbox input {
		padding-left: 2.1rem;
	}
	.empty {
		color: var(--muted);
	}
	section {
		margin-bottom: 2.25rem;
	}
	h2 {
		font-size: 1.05rem;
		margin: 0 0 0.75rem;
		padding-bottom: 0.35rem;
		border-bottom: 1px solid var(--border);
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr));
		gap: 1.25rem;
	}
	.card {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		color: var(--fg);
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
	.title {
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--fg);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	a.title:hover {
		color: var(--accent);
		text-decoration: none;
	}
	.sub {
		font-size: 0.8rem;
		color: var(--muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.context {
		font-size: 0.75rem;
		color: var(--gold);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.import {
		font-size: 0.78rem;
		padding: 0.3rem 0.6rem;
		justify-content: center;
		margin-top: 0.15rem;
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
	.shelfgrid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
		gap: 1rem;
	}
	.shelfcard {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
		padding: 0.5rem 0.75rem;
		color: var(--fg);
		box-shadow: var(--shadow);
	}
	.shelfcard:hover {
		text-decoration: none;
		border-color: var(--accent);
	}
	.shelfcover {
		width: 3.2rem;
		height: 3.2rem;
		border-radius: 6px;
		overflow: hidden;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg);
		color: var(--gold);
		border: 1px solid var(--border);
	}
	.shelfcover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.shelfmeta {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.subrow {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
	}
	.name {
		font-weight: 600;
		font-size: 0.95rem;
	}
</style>
