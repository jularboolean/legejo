<script lang="ts">
	import { Search } from '@lucide/svelte';
	import { getLocale, t } from '#lib/i18n';
	import { collectAuthors, fold, initial, type AuthorEntry } from '#lib/library/authors';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	let query = $state('');

	const all = $derived(collectAuthors(data.books, getLocale()));
	const shown = $derived.by(() => {
		const q = fold(query.trim());
		return q ? all.filter((a) => fold(a.name).includes(q)) : all;
	});
	const groups = $derived.by(() => {
		const out: { letter: string; authors: AuthorEntry[] }[] = [];
		for (const a of shown) {
			const letter = initial(a.sort, getLocale());
			const last = out[out.length - 1];
			if (last?.letter === letter) last.authors.push(a);
			else out.push({ letter, authors: [a] });
		}
		return out;
	});
	const withoutAuthor = $derived(data.books.filter((b) => !b.author?.trim()).length);

	function cover(book: AuthorEntry['books'][number]): string {
		return `/api/books/${book.id}/cover?size=thumb&v=${encodeURIComponent(book.updated_at ?? book.created_at)}`;
	}
</script>

<header>
	<h1>{t('authors.heading')}</h1>
	<p class="summary">
		{t('authors.count', { count: all.length })} · {t('authors.books', { count: data.books.length })}
		{#if withoutAuthor > 0}· {t('authors.withoutAuthor', { count: withoutAuthor })}{/if}
	</p>
</header>

{#if all.length === 0}
	<p class="empty">{t('authors.empty')}</p>
{:else}
	<div class="tools">
		<div class="search">
			<Search size={14} class="search-icon" aria-hidden="true" />
			<input type="search" bind:value={query} placeholder={t('library.filter.authorSearch')} />
		</div>
		{#if !query.trim()}
			<nav class="letters" aria-label={t('authors.letters')}>
				{#each groups as g (g.letter)}
					<a href={`#letter-${g.letter}`}>{g.letter}</a>
				{/each}
			</nav>
		{/if}
	</div>

	{#each groups as g (g.letter)}
		<section id={`letter-${g.letter}`}>
			<h2>{g.letter}</h2>
			<ul>
				{#each g.authors as a (a.key)}
					<li>
						<a class="row" href={`/?author=${encodeURIComponent(a.key)}`}>
							<span class="who">
								<span class="name">{a.name}</span>
								<span class="meta">
									{t('authors.books', { count: a.books.length })}{#if a.finished > 0}
										· {t('authors.finished', { count: a.finished })}{/if}
								</span>
							</span>
							<span class="covers" aria-hidden="true">
								{#each a.books.filter((b) => b.has_cover).slice(0, 5) as book (book.id)}
									<img src={cover(book)} alt="" loading="lazy" />
								{/each}
							</span>
						</a>
					</li>
				{/each}
			</ul>
		</section>
	{:else}
		<p class="empty">{t('authors.noMatch')}</p>
	{/each}
{/if}

<style>
	header {
		margin-bottom: 1rem;
	}
	h1 {
		font-size: 1.4rem;
		margin: 0 0 0.25rem;
	}
	.summary {
		margin: 0;
		color: var(--muted);
		font-size: 0.88rem;
	}
	.tools {
		position: sticky;
		top: 0;
		z-index: 5;
		background: var(--bg);
		padding: 0.5rem 0 0.6rem;
		margin-bottom: 0.5rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		border-bottom: 1px solid var(--border);
	}
	.search {
		position: relative;
		max-width: 22rem;
	}
	.search :global(.search-icon) {
		position: absolute;
		left: 0.7rem;
		top: 50%;
		transform: translateY(-50%);
		color: var(--muted);
		pointer-events: none;
	}
	.search input {
		padding-left: 2rem;
	}
	.letters {
		display: flex;
		flex-wrap: wrap;
		gap: 0.15rem 0.2rem;
	}
	.letters a {
		min-width: 1.6rem;
		text-align: center;
		padding: 0.1rem 0.3rem;
		border-radius: 5px;
		font-size: 0.82rem;
		font-weight: 600;
		color: var(--muted);
	}
	.letters a:hover {
		background: var(--card);
		color: var(--fg);
		text-decoration: none;
	}
	section {
		scroll-margin-top: 6.5rem;
		max-width: 46rem;
	}
	h2 {
		font-family: Georgia, serif;
		font-size: 1.15rem;
		margin: 1.1rem 0 0.3rem;
		color: var(--accent);
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	li + li {
		border-top: 1px solid var(--border);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0.5rem 0.5rem;
		margin: 0 -0.5rem;
		border-radius: 6px;
		color: var(--fg);
	}
	.row:hover {
		background: var(--card);
		text-decoration: none;
	}
	.who {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	.name {
		font-weight: 600;
		font-size: 0.95rem;
	}
	.meta {
		font-size: 0.78rem;
		color: var(--muted);
	}
	.covers {
		display: flex;
		flex-shrink: 0;
	}
	.covers img {
		width: 1.9rem;
		height: 2.85rem;
		object-fit: cover;
		border-radius: 2px 3px 3px 2px;
		border: 1px solid var(--border);
		box-shadow: var(--shadow);
		background: var(--card);
	}
	.covers img + img {
		margin-left: -0.6rem;
	}
	.empty {
		color: var(--muted);
		font-style: italic;
	}
	@media (max-width: 30rem) {
		.covers img:nth-child(n + 4) {
			display: none;
		}
	}
</style>
