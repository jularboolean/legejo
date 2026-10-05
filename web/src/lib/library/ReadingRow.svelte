<script lang="ts">
	import { t } from '#lib/i18n';
	import type { Book } from '#lib/types';

	let {
		title,
		books,
		total,
		onall
	}: {
		title: string;
		books: Book[];
		/** All books in the row's category; "show all" when more than shown. */
		total: number;
		onall: () => void;
	} = $props();
</script>

<section class="strip">
	<header>
		<h2>{title} <span class="n">{total}</span></h2>
		{#if total > books.length}
			<button type="button" class="all" onclick={onall}>{t('home.showAll')}</button>
		{/if}
	</header>
	<ul>
		{#each books as book (book.id)}
			<li>
				<a href={`/books/${book.id}`} title={book.title}>
					<span class="cover">
						{#if book.has_cover}
							<img
								src={`/api/books/${book.id}/cover?size=thumb&v=${encodeURIComponent(book.updated_at ?? book.created_at)}`}
								alt=""
								loading="lazy"
							/>
						{:else}
							<span class="placeholder">{book.title}</span>
						{/if}
						{#if book.progress_percent != null}
							{@const percent = Math.round(book.progress_percent * 100)}
							<span class="bar" title={t('book.progress', { percent })}><span style:width={`${percent}%`}></span></span>
						{/if}
					</span>
					<span class="title">{book.title}</span>
				</a>
			</li>
		{/each}
	</ul>
</section>

<style>
	.strip {
		margin-bottom: 1.5rem;
	}
	header {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
		margin-bottom: 0.5rem;
	}
	h2 {
		margin: 0;
		font-size: 0.78rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--muted);
	}
	.n {
		font-weight: 400;
		margin-left: 0.25rem;
	}
	.all {
		background: none;
		border: none;
		padding: 0;
		color: var(--accent);
		font-size: 0.82rem;
		font-weight: 500;
	}
	.all:hover {
		text-decoration: underline;
		filter: none;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0 0 0.4rem;
		display: flex;
		gap: 0.9rem;
		overflow-x: auto;
		scroll-snap-type: x proximity;
		scrollbar-width: thin;
	}
	li {
		flex: 0 0 6.5rem;
		scroll-snap-align: start;
	}
	a {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		color: var(--fg);
	}
	a:hover {
		text-decoration: none;
	}
	.cover {
		position: relative;
		display: block;
		aspect-ratio: 2 / 3;
		border-radius: 3px 6px 6px 3px;
		overflow: hidden;
		border: 1px solid var(--border);
		box-shadow: var(--shadow);
		background: var(--card);
	}
	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	a:hover .cover {
		border-color: var(--accent);
	}
	.placeholder {
		display: grid;
		place-items: center;
		height: 100%;
		padding: 0.4rem;
		font-size: 0.7rem;
		text-align: center;
		color: var(--muted);
	}
	.bar {
		position: absolute;
		inset: auto 0 0 0;
		height: 4px;
		background: rgb(0 0 0 / 0.35);
	}
	.bar span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.title {
		font-size: 0.78rem;
		line-height: 1.25;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
</style>
