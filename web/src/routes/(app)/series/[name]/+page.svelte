<script lang="ts">
	import BookCover from '#lib/library/BookCover.svelte';
	import { ArrowLeft, BookOpen, Check, Layers } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import { readStatus, seriesProgress } from '#lib/library';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const progress = $derived(seriesProgress(data.seriesBooks));
	// Many missing numbers read as a list too long to help; name the first few.
	const shownGaps = $derived(progress.gaps.slice(0, 8));

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

{#if data.seriesBooks.length > 0}
	<div class="overview">
		<div class="tally">
			<span class="bar" aria-hidden="true"><span style:width={`${Math.round((progress.finished / progress.total) * 100)}%`}></span></span>
			<span>{t('series.finished', { finished: progress.finished, total: progress.total })}</span>
		</div>
		{#if progress.next}
			{@const next = progress.next}
			<a class="button next" href={`/books/${next.id}/read`}>
				<BookOpen size={14} />
				{readStatus(next) === 'reading' ? t('series.continue') : t('series.start')}
				<span class="next-title">{indexLabel(next.series_index) ?? ''} {next.title}</span>
			</a>
		{:else}
			<span class="done"><Check size={14} /> {t('series.allRead')}</span>
		{/if}
		{#if shownGaps.length > 0}
			<p class="gaps">
				{t('series.gaps', {
					numbers: shownGaps.map((n) => `#${n}`).join(', ') + (progress.gaps.length > shownGaps.length ? ' …' : '')
				})}
			</p>
		{/if}
	</div>
{/if}

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
						<BookCover title={book.title} author={book.author} format={book.format} />
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
	.overview {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.6rem 1.25rem;
		margin: -0.5rem 0 1.75rem;
	}
	.tally {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.tally .bar {
		width: 6rem;
		height: 5px;
		border-radius: 999px;
		background: var(--border);
		overflow: hidden;
	}
	.tally .bar span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.next {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		max-width: 100%;
	}
	.next-title {
		font-weight: 400;
		opacity: 0.85;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		max-width: 16rem;
	}
	.done {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.85rem;
		color: var(--accent);
	}
	.gaps {
		flex-basis: 100%;
		margin: 0;
		font-size: 0.8rem;
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
