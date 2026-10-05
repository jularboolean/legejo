<script lang="ts">
	import { formatPublished } from '#lib/pubdate';
	import { invalidateAll } from '$app/navigation';
	import { Check, ChevronRight, Download, Globe } from '@lucide/svelte';
	import Avatar from '#lib/Avatar.svelte';
	import { getLocale, t } from '#lib/i18n';
	import { renderMarkdown } from '#lib/markdown';
	import Rating from '#lib/Rating.svelte';
	import { defaultShelfCover } from '#lib/shelfCovers';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const shelf = $derived(data.publicShelfRef);
	const book = $derived(data.publicBook);
	const shelfPath = $derived(
		`/public/${encodeURIComponent(shelf.owner)}/${encodeURIComponent(shelf.name)}`
	);
	const descriptionHtml = $derived(book.description ? renderMarkdown(book.description) : '');

	let importing = $state(false);
	let imported = $state(false);
	let errorMsg = $state('');

	async function importBook() {
		importing = true;
		errorMsg = '';
		try {
			const res = await fetch(`/api/public/books/${book.id}/import`, { method: 'POST' });
			if (res.ok) {
				imported = true;
				await invalidateAll();
			} else {
				const body = await res.json().catch(() => null);
				errorMsg = body?.error ?? t('public.importFailed');
			}
		} catch {
			errorMsg = t('common.network');
		} finally {
			importing = false;
		}
	}

	function formatSize(bytes: number): string {
		if (bytes >= 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + ' MB';
		return Math.max(1, Math.round(bytes / 1024)) + ' kB';
	}
</script>

<nav class="crumbs" aria-label={t('public.breadcrumb')}>
	<a href="/public"><Globe size={13} /> {t('public.heading')}</a>
	<ChevronRight size={13} aria-hidden="true" />
	<a href={shelfPath}>{shelf.name}</a>
	<ChevronRight size={13} aria-hidden="true" />
	<span class="current" title={book.title}>{book.title}</span>
</nav>

<a class="hero has-image" href={shelfPath}>
	<img
		class="hero-image"
		src={shelf.has_cover ? `/api/public/shelves/${shelf.id}/cover` : defaultShelfCover(shelf.id)}
		alt=""
	/>
	<div class="hero-shade"></div>
	<div class="hero-content">
		<span class="hero-name">{shelf.name}</span>
		<span class="hero-meta">
			<Avatar userId={shelf.owner_id} hasAvatar={shelf.owner_has_avatar} size={18} alt="" />
			{t('public.by', { owner: shelf.owner })} · {shelf.book_count}
		</span>
	</div>
</a>

<div class="detail">
	<div class="side">
		<div class="cover">
			{#if book.has_cover}
				<img src={`/api/public/books/${book.id}/cover?v=${encodeURIComponent(book.updated_at ?? book.created_at)}`} alt={book.title} />
			{:else}
				<span class="placeholder">{book.title}</span>
			{/if}
		</div>
		{#if book.owned || imported}
			<span class="done"><Check size={14} /> {imported ? t('public.imported') : t('public.owned')}</span>
		{:else}
			<button class="import" disabled={importing} onclick={importBook}>
				<Download size={14} />
				{importing ? t('public.importing') : t('public.import')}
			</button>
			<span class="hint">{t('public.importHint')}</span>
		{/if}
		{#if errorMsg}<p class="error">{errorMsg}</p>{/if}
	</div>

	<div class="info">
		<h1>{book.title}</h1>
		{#if book.author}<p class="author">{book.author}</p>{/if}
		{#if book.rating}<p class="rating"><Rating value={book.rating} size={24} /></p>{/if}

		{#if book.category}
			<div class="chips"><span class="chip category">{book.category}</span></div>
		{/if}

		{#if descriptionHtml}
			<div class="description">{@html descriptionHtml}</div>
		{:else}
			<p class="nodesc">{t('book.noDescription')}</p>
		{/if}

		<dl>
			{#if book.language}<dt>{t('book.language')}</dt><dd>{book.language}</dd>{/if}
			{#if book.first_published != null}<dt>{t('book.firstPublished')}</dt><dd>{book.first_published}</dd>{/if}
			{#if book.published}<dt>{t('book.edition')}</dt><dd>{formatPublished(book.published, getLocale())}</dd>{/if}
			{#if book.publisher}<dt>{t('book.publisher')}</dt><dd>{book.publisher}</dd>{/if}
			{#if book.isbn}<dt>{t('book.isbn')}</dt><dd>{book.isbn}</dd>{/if}
			<dt>{t('book.fileSize')}</dt><dd>{formatSize(book.file_size)}</dd>
		</dl>
	</div>
</div>

<style>
	.crumbs {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		margin-bottom: 1rem;
		font-size: 0.85rem;
		color: var(--muted);
		min-width: 0;
	}
	.crumbs a {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		color: var(--muted);
	}
	.crumbs a:hover {
		color: var(--accent);
		text-decoration: none;
	}
	.current {
		color: var(--fg);
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.hero {
		position: relative;
		display: block;
		border-radius: 12px;
		overflow: hidden;
		margin-bottom: 1.5rem;
		color: inherit;
	}
	.hero:hover {
		text-decoration: none;
	}
	.hero.has-image {
		min-height: 5.5rem;
		display: flex;
		align-items: flex-end;
	}
	.hero:not(.has-image) {
		border: 1px solid var(--border);
		background: var(--card);
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
		background: linear-gradient(to top, rgba(0, 0, 0, 0.6) 0%, rgba(0, 0, 0, 0.1) 60%, transparent);
	}
	.hero-content {
		position: relative;
		width: 100%;
		display: flex;
		align-items: baseline;
		gap: 0.6rem;
		padding: 0.6rem 1rem;
	}
	.has-image .hero-content {
		color: #fff;
	}
	.hero-name {
		font-family: 'Fraunces', Georgia, serif;
		font-weight: 700;
		font-size: 1.05rem;
	}
	.has-image .hero-name {
		text-shadow: 0 1px 4px rgba(0, 0, 0, 0.4);
	}
	.hero-meta {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.8rem;
		color: var(--muted);
	}
	.has-image .hero-meta {
		color: rgba(255, 255, 255, 0.85);
	}
	.detail {
		display: grid;
		grid-template-columns: 14rem 1fr;
		gap: 2rem;
		align-items: start;
	}
	@media (max-width: 40rem) {
		.detail {
			grid-template-columns: 1fr;
		}
		.side {
			max-width: 14rem;
		}
	}
	.side {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	.cover {
		aspect-ratio: 2 / 3;
		border-radius: 3px 8px 8px 3px;
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
		padding: 1rem;
		text-align: center;
		color: var(--muted);
	}
	.import {
		justify-content: center;
	}
	.done {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.35rem;
		color: var(--accent);
		font-size: 0.9rem;
		font-weight: 600;
	}
	.hint {
		font-size: 0.78rem;
		color: var(--muted);
		text-align: center;
	}
	h1 {
		margin: 0;
		font-size: 1.5rem;
	}
	.rating {
		margin: 0.5rem 0 0;
	}
	.author {
		margin: 0.25rem 0 0;
		color: var(--muted);
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		margin-top: 0.9rem;
	}
	.chip {
		font-size: 0.8rem;
		padding: 0.15rem 0.6rem;
		border-radius: 99px;
		border: 1px solid var(--border);
		color: var(--muted);
	}
	.chip.category {
		border-color: var(--accent);
		color: var(--accent);
	}
	.description {
		margin-top: 1.25rem;
		max-width: 42rem;
	}
	.description :global(p:first-child) {
		margin-top: 0;
	}
	.nodesc {
		margin-top: 1.25rem;
		color: var(--muted);
		font-style: italic;
	}
	dl {
		margin-top: 1.5rem;
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 0.3rem 1.25rem;
		font-size: 0.9rem;
	}
	dt {
		color: var(--muted);
	}
	dd {
		margin: 0;
	}
</style>
