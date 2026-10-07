<script lang="ts" generics="T extends Book & { shelf_ids?: number[] }">
	import { BookMarked, Check, Download, Layers } from '@lucide/svelte';
	import type { Snippet } from 'svelte';
	import { getLocale, t } from '#lib/i18n';
	import { bookYear, languageName, plainSnippet } from '#lib/library';
	import Rating from '#lib/Rating.svelte';
	import type { Book, Shelf } from '#lib/types';

	let {
		books,
		shelves = [],
		showLanguage = false,
		href = (book: T) => `/books/${book.id}`,
		cover = (book: T) =>
			`/api/books/${book.id}/cover?size=thumb&v=${encodeURIComponent(book.updated_at ?? book.created_at)}`,
		own = true,
		context,
		action,
		selected = null,
		ontoggle
	}: {
		books: T[];
		/** Name each book's language in the byline. */
		showLanguage?: boolean;
		/** For the shelf chips; rows without shelf_ids show none. */
		shelves?: Shelf[];
		href?: (book: T) => string;
		cover?: (book: T) => string;
		/** The caller's own books: download link and a link to the series page. */
		own?: boolean;
		/** An extra line under the byline, e.g. where a public book was found. */
		context?: Snippet<[T]>;
		/** Shown to the right, e.g. an import button. */
		action?: Snippet<[T]>;
		/** Selection mode when set: a tap selects instead of opening the book. */
		selected?: Set<number> | null;
		ontoggle?: (id: number) => void;
	} = $props();

	const shelfNames = $derived(new Map(shelves.map((s) => [s.id, s.name])));

	function seriesLabel(book: T): string {
		if (book.series_index == null) return book.series ?? '';
		return `${book.series} #${book.series_index}`;
	}
</script>

<ul class="rows">
	{#each books as book (book.id)}
		{@const year = bookYear(book)}
		{@const snippet = plainSnippet(book.description)}
		{@const shelfIds = (book.shelf_ids ?? []).filter((id) => shelfNames.has(id))}
		<li class="row" class:picked={selected?.has(book.id)}>
			{#if selected}
				<button
					type="button"
					class="pick"
					aria-pressed={selected.has(book.id)}
					aria-label={book.title}
					onclick={() => ontoggle?.(book.id)}
				>
					<span class="tick">{#if selected.has(book.id)}<Check size={14} strokeWidth={3} />{/if}</span>
				</button>
			{/if}
			<a class="cover" href={href(book)} tabindex="-1" aria-hidden="true">
				{#if book.has_cover}
					<img src={cover(book)} alt="" loading="lazy" />
				{/if}
			</a>

			<div class="main">
				<a class="title" href={href(book)}>{book.title}</a>
				<p class="byline">
					{#if book.author}<span>{book.author}</span>{/if}
					{#if year}<span>{year}</span>{/if}
					{#if showLanguage && languageName(book.language, getLocale())}
						<span>{languageName(book.language, getLocale())}</span>
					{/if}
					{#if book.series && own}
						<a class="series" href={`/series/${encodeURIComponent(book.series)}`}>
							<Layers size={12} />
							{seriesLabel(book)}
						</a>
					{:else if book.series}
						<span class="series"><Layers size={12} /> {seriesLabel(book)}</span>
					{/if}
					{#if book.rating}<span class="rated"><Rating value={book.rating} size={17} /></span>{/if}
				</p>
				{#if context}<p class="context">{@render context(book)}</p>{/if}
				{#if snippet}<p class="snippet">{snippet}</p>{/if}
				{#if shelfIds.length > 0}
					<div class="chips">
						{#each shelfIds as id (id)}
							<a class="chip" href={`/shelves/${id}`}>
								<BookMarked size={11} />
								{shelfNames.get(id)}
							</a>
						{/each}
					</div>
				{/if}
			</div>

			<div class="aside" class:shown={book.progress_percent != null || action !== undefined}>
				{#if book.progress_percent != null}
					{@const percent = Math.round(book.progress_percent * 100)}
					<span class="percent" title={t('book.progress', { percent })}>{percent} %</span>
					<span class="bar" aria-hidden="true"><span style:width={`${percent}%`}></span></span>
				{/if}
				{#if action}{@render action(book)}{/if}
				{#if own}
					<a
						class="download"
						href={`/api/books/${book.id}/file`}
						title={t('home.download')}
						aria-label={t('home.download')}
					>
						<Download size={14} />
					</a>
				{/if}
			</div>
		</li>
	{/each}
</ul>

<style>
	.pick {
		position: absolute;
		inset: 0;
		z-index: 3;
		background: transparent;
		border: 2px solid transparent;
		border-radius: 8px;
		padding: 0;
		cursor: pointer;
	}
	.pick:hover {
		filter: none;
		border-color: var(--border);
	}
	.picked .pick {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 8%, transparent);
	}
	.tick {
		position: absolute;
		top: 50%;
		right: 0.75rem;
		transform: translateY(-50%);
		width: 1.4rem;
		height: 1.4rem;
		border-radius: 50%;
		display: grid;
		place-items: center;
		background: var(--card);
		border: 1.5px solid var(--muted);
		color: var(--bg);
	}
	.picked .tick {
		background: var(--accent);
		border-color: var(--accent);
	}
	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.row {
		position: relative;
		display: grid;
		grid-template-columns: 4.5rem minmax(0, 1fr) auto;
		gap: 1.1rem;
		align-items: start;
		padding: 0.9rem 0.75rem;
		margin: 0 -0.75rem;
		border-radius: 10px;
		transition: background 0.12s;
	}
	/* A straight hairline between rows, inset to the text column's edges; a
	   border on the row itself would follow its rounded corners. */
	.row + .row::before {
		content: '';
		position: absolute;
		top: 0;
		left: 0.75rem;
		right: 0.75rem;
		height: 1px;
		background: var(--border);
	}
	.row:hover {
		background: var(--card);
	}
	.row:hover::before,
	.row:hover + .row::before {
		display: none;
	}
	.cover {
		aspect-ratio: 2 / 3;
		border-radius: 2px 5px 5px 2px;
		overflow: hidden;
		background: var(--card);
		border: 1px solid var(--border);
		box-shadow: var(--shadow);
		display: block;
	}
	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.main {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.title {
		font-family: 'Fraunces', Georgia, serif;
		font-size: 1.08rem;
		font-weight: 600;
		line-height: 1.25;
		color: var(--fg);
	}
	.title:hover {
		color: var(--accent);
		text-decoration: none;
	}
	.byline {
		margin: 0;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.byline > * + *::before {
		content: '·';
		margin: 0 0.45rem;
		color: var(--border);
	}
	.series {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		color: var(--gold);
	}
	.rated {
		display: inline-flex;
		align-items: center;
	}
	a.series:hover {
		color: var(--accent);
		text-decoration: none;
	}
	.context {
		margin: 0;
		font-size: 0.8rem;
		color: var(--gold);
	}
	.snippet {
		margin: 0.1rem 0 0;
		font-size: 0.88rem;
		line-height: 1.5;
		color: var(--muted);
		max-width: 46rem;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
		margin-top: 0.3rem;
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.75rem;
		padding: 0.1rem 0.55rem;
		border-radius: 99px;
		border: 1px solid var(--gold);
		color: var(--gold);
	}
	.chip:hover {
		text-decoration: none;
		background: var(--bg);
	}
	.aside {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 0.3rem;
		min-width: 4.5rem;
		padding-top: 0.15rem;
	}
	.percent {
		font-size: 0.8rem;
		font-variant-numeric: tabular-nums;
		color: var(--muted);
	}
	.bar {
		width: 4.5rem;
		height: 3px;
		border-radius: 2px;
		background: var(--border);
		overflow: hidden;
	}
	.bar span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.download {
		display: inline-flex;
		padding: 0.3rem;
		border-radius: 6px;
		color: var(--muted);
		opacity: 0;
		transition: opacity 0.12s;
	}
	.download:hover {
		color: var(--fg);
		background: var(--bg);
	}
	.row:hover .download,
	.download:focus-visible {
		opacity: 1;
	}
	@media (max-width: 40rem) {
		.row {
			grid-template-columns: 3.5rem minmax(0, 1fr);
			gap: 0.8rem;
		}
		/* No hover on touch screens: the download lives on the book's page,
		   and a row without progress needs no third line at all. */
		.aside {
			display: none;
		}
		.aside.shown {
			display: flex;
			grid-column: 2;
			flex-direction: row;
			align-items: center;
			min-width: 0;
		}
		.download {
			display: none;
		}
	}
</style>
