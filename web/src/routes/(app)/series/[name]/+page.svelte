<script lang="ts">
	import { ArrowLeft, Layers } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	function indexLabel(index: number | null): string | null {
		if (index == null) return null;
		return `#${Number.isInteger(index) ? String(index) : String(index)}`;
	}
</script>

<a class="back" href="/"><ArrowLeft size={13} /> {t('book.back')}</a>

<div class="headline">
	<h1><Layers size={20} strokeWidth={1.75} /> {data.seriesName}</h1>
	<span class="count">{data.seriesBooks.length}</span>
</div>

{#if data.seriesBooks.length === 0}
	<p class="empty">{t('series.empty')}</p>
{:else}
	<div class="grid">
		{#each data.seriesBooks as book (book.id)}
			<div class="card">
				<a class="cover" href={`/books/${book.id}`}>
					{#if book.has_cover}
						<img src={`/api/books/${book.id}/cover?size=thumb&v=${encodeURIComponent(book.updated_at ?? book.created_at)}`} alt={book.title} loading="lazy" />
					{:else}
						<span class="placeholder">{book.title}</span>
					{/if}
					{#if indexLabel(book.series_index)}
						<span class="index">{indexLabel(book.series_index)}</span>
					{/if}
					{#if book.progress_percent != null}
						{@const percent = Math.round(book.progress_percent * 100)}
						<span class="read-progress" title={t('book.progress', { percent })}>
							<span style:width={`${percent}%`}></span>
						</span>
					{/if}
				</a>
				<div class="meta">
					<span class="title" title={book.title}>{book.title}</span>
					{#if book.author}<span class="author">{book.author}</span>{/if}
				</div>
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
	.headline {
		display: flex;
		align-items: baseline;
		gap: 0.6rem;
		margin-bottom: 1.5rem;
	}
	h1 {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 1.4rem;
		margin: 0;
	}
	h1 :global(svg) {
		color: var(--gold);
	}
	.count {
		color: var(--muted);
		font-size: 1rem;
	}
	.empty {
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
		position: relative;
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
	.index {
		position: absolute;
		top: 0.4rem;
		left: 0.4rem;
		font-size: 0.75rem;
		font-weight: 700;
		color: var(--bg);
		background: var(--gold);
		border-radius: 99px;
		padding: 0.05rem 0.5rem;
	}
	.read-progress {
		position: absolute;
		inset: auto 0 0 0;
		height: 4px;
		background: rgba(0, 0, 0, 0.35);
	}
	.read-progress span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.meta {
		display: flex;
		flex-direction: column;
	}
	.title {
		font-size: 0.9rem;
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.author {
		font-size: 0.8rem;
		color: var(--muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
</style>
