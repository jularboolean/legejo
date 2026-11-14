<script lang="ts">
	import BookCover from '#lib/library/BookCover.svelte';
	import BookGrid from '#lib/library/BookGrid.svelte';
	import { Layers } from '@lucide/svelte';
	import { getLocale, t } from '#lib/i18n';
	import { cover } from '#lib/cover';
	import { groupBySeries, readStatus } from '#lib/library';
	import type { LibraryBook } from '#lib/types';

	// My library by series: one card per series, opening the series page, and
	// the books in no series after them as usual.
	let { books }: { books: LibraryBook[] } = $props();

	const grouped = $derived(groupBySeries(books, getLocale()));

	function authorsOf(list: LibraryBook[]): string | null {
		const names = [...new Set(list.map((b) => b.author).filter((a): a is string => !!a))];
		if (names.length === 0) return null;
		return names.length === 1 ? names[0] : `${names[0]} …`;
	}
</script>

{#if grouped.series.length > 0}
	<div class="grid">
		{#each grouped.series as group (group.name)}
			{@const book = group.cover}
			{@const finished = group.books.filter((b) => readStatus(b) === 'finished').length}
			<a class="card" href={`/series/${encodeURIComponent(group.name)}`}>
				<span class="pile" class:many={group.books.length > 1}>
					<span class="cover">
						{#if book.has_cover}
							<img
								use:cover
								src={`/api/books/${book.id}/cover?size=thumb&v=${encodeURIComponent(book.updated_at ?? book.created_at)}`}
								alt=""
								loading="lazy"
							/>
						{:else}
							<BookCover title={group.name} author={book.author} format={book.format} />
						{/if}
						<span class="count" title={t('library.series.count', { count: group.books.length })}>
							<Layers size={11} strokeWidth={2.25} />
							{group.books.length}
						</span>
						{#if finished > 0}
							<span class="read-progress">
								<span style:width={`${Math.round((finished / group.books.length) * 100)}%`}></span>
							</span>
						{/if}
					</span>
				</span>
				<span class="meta">
					<span class="title" title={group.name}>{group.name}</span>
					{#if authorsOf(group.books)}<span class="author">{authorsOf(group.books)}</span>{/if}
				</span>
			</a>
		{/each}
	</div>
{/if}

{#if grouped.loose.length > 0}
	{#if grouped.series.length > 0}
		<h2 class="loose">{t('library.series.none')} <span class="n">{grouped.loose.length}</span></h2>
	{/if}
	<BookGrid books={grouped.loose} />
{/if}

<style>
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr));
		gap: 1.25rem;
	}
	.card {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		color: inherit;
		text-decoration: none;
	}
	/* A series reads as a small pile: two edges behind the cover. */
	.pile {
		position: relative;
		display: block;
	}
	.pile.many::before,
	.pile.many::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: 3px 6px 6px 3px;
		background: var(--card);
		border: 1px solid var(--border);
	}
	.pile.many::before {
		transform: translate(6px, -6px);
		opacity: 0.55;
	}
	.pile.many::after {
		transform: translate(3px, -3px);
		opacity: 0.85;
	}
	.cover {
		position: relative;
		z-index: 1;
		aspect-ratio: 2 / 3;
		border-radius: 3px 6px 6px 3px;
		overflow: hidden;
		background: var(--card);
		border: 1px solid var(--border);
		display: flex;
		box-shadow: var(--shadow);
		transition: transform 0.15s ease;
	}
	.card:hover .cover {
		transform: translateY(-2px);
	}
	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.count {
		position: absolute;
		top: 0.4rem;
		right: 0.4rem;
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		padding: 0.08rem 0.4rem;
		border-radius: 999px;
		background: color-mix(in srgb, var(--fg) 78%, transparent);
		color: var(--bg);
		font-size: 0.7rem;
		font-weight: 700;
		font-variant-numeric: tabular-nums;
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
	.loose {
		margin: 2rem 0 1rem;
		font-size: 1rem;
		font-weight: 600;
	}
	.loose .n {
		color: var(--muted);
		font-weight: 400;
		font-size: 0.9rem;
		margin-left: 0.3rem;
	}
</style>
