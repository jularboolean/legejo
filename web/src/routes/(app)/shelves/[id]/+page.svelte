<script lang="ts">
	import { Pencil } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import { renderMarkdown } from '#lib/markdown';
	import { defaultShelfCover } from '#lib/shelfCovers';
	import { loadView, saveView, type ViewMode } from '#lib/library';
	import BookGrid from '#lib/library/BookGrid.svelte';
	import BookRows from '#lib/library/BookRows.svelte';
	import ViewSwitch from '#lib/library/ViewSwitch.svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const shelf = $derived(data.shelf);
	const descriptionHtml = $derived(shelf.description ? renderMarkdown(shelf.description) : '');

	// Shared with the library and the search page.
	let view = $state<ViewMode>(loadView());

	function setView(next: ViewMode) {
		view = next;
		saveView(next);
	}
</script>

<header class="hero has-image">
	<img
		class="hero-image"
		src={shelf.has_cover ? `/api/shelves/${shelf.id}/cover` : defaultShelfCover(shelf.id)}
		alt=""
	/>
	<div class="hero-shade"></div>
	<div class="hero-content">
		<div class="headline">
			<h1>{shelf.name}</h1>
			<a
				class="edit-btn"
				href={`/shelves/${shelf.id}/edit`}
				title={t('shelf.edit')}
				aria-label={t('shelf.edit')}
			>
				<Pencil size={15} />
			</a>
		</div>
		<p class="meta">
			{shelf.book_count === 1 ? t('shelf.book') : t('shelf.books', { count: shelf.book_count })}
			{#if shelf.visibility === 'federated'}
				· <span class="public">{t('shelf.federated')}</span>
				{#if shelf.handle}<span class="handle">{shelf.handle}</span>{/if}
			{:else if shelf.is_public}
				· <span class="public">{t('shelf.public')}</span>
			{/if}
		</p>
	</div>
</header>

<div class="intro">
	{#if descriptionHtml}
		<div class="description">{@html descriptionHtml}</div>
	{/if}
	{#if shelf.books.length > 0}
		<div class="switch"><ViewSwitch {view} onchange={setView} /></div>
	{/if}
</div>

{#if shelf.books.length === 0}
	<p class="empty">{t('shelf.empty')}</p>
{:else if view === 'list'}
	<BookRows books={shelf.books} />
{:else}
	<BookGrid books={shelf.books} />
{/if}

<style>
	.hero {
		position: relative;
		border-radius: 12px;
		overflow: hidden;
		margin-bottom: 1.5rem;
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
	.has-image .edit-btn {
		color: rgba(255, 255, 255, 0.75);
	}
	.has-image .edit-btn:hover {
		color: #fff;
		background: rgba(0, 0, 0, 0.3);
	}
	.headline {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
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
		margin: 0.25rem 0 0;
		color: var(--muted);
		font-size: 0.9rem;
	}
	.public {
		color: inherit;
	}
	.handle {
		margin-left: 0.35rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		font-size: 0.8rem;
		overflow-wrap: anywhere;
	}
	.edit-btn {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		padding: 0.35rem;
		border-radius: 6px;
		color: var(--muted);
	}
	.edit-btn:hover {
		text-decoration: none;
		color: var(--fg);
		background: var(--card);
	}
	.intro {
		display: flex;
		align-items: flex-start;
		gap: 1.5rem;
		margin: 0.25rem 0 1.25rem;
	}
	.switch {
		margin-left: auto;
	}
	.description {
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
	.empty {
		color: var(--muted);
	}
</style>
