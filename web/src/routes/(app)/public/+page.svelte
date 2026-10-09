<script lang="ts">
	import { Users } from '@lucide/svelte';
	import Avatar from '#lib/Avatar.svelte';
	import ViewSwitch from '#lib/library/ViewSwitch.svelte';
	import { plainSnippet, type ViewMode } from '#lib/library';
	import { defaultShelfCover } from '#lib/shelfCovers';
	import { t } from '#lib/i18n';
	import type { PublicShelf } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	// Gallery or list, remembered per device like the library's own view.
	const VIEW_KEY = 'legejo.public.view';
	let view = $state<ViewMode>('grid');
	$effect(() => {
		try {
			const saved = localStorage.getItem(VIEW_KEY);
			if (saved === 'grid' || saved === 'list') view = saved;
		} catch {
			// No storage: the gallery it is.
		}
	});
	function setView(next: ViewMode) {
		view = next;
		try {
			localStorage.setItem(VIEW_KEY, next);
		} catch {
			// The choice just doesn't stick.
		}
	}

	const shelfHref = (shelf: PublicShelf) =>
		`/public/${encodeURIComponent(shelf.owner)}/${encodeURIComponent(shelf.name)}`;
	const shelfCover = (shelf: PublicShelf) =>
		shelf.has_cover ? `/api/public/shelves/${shelf.id}/cover` : defaultShelfCover(shelf.id);
	const count = (shelf: PublicShelf) =>
		shelf.book_count === 1 ? t('shelf.book') : t('shelf.books', { count: shelf.book_count });
</script>

<div class="head">
	<h1>{t('public.heading')}</h1>
	{#if data.publicShelves.length > 0}
		<ViewSwitch {view} onchange={setView} />
	{/if}
</div>

{#if data.publicShelves.length === 0}
	<p class="empty">{t('public.empty')}</p>
{:else if view === 'list'}
	<ul class="rows">
		{#each data.publicShelves as shelf (shelf.id)}
			<li>
				<a class="row" href={shelfHref(shelf)}>
					<img class="thumb" src={shelfCover(shelf)} alt="" loading="lazy" />
					<div class="info">
						<span class="name">{shelf.name}</span>
						<span class="owner">
							<Avatar userId={shelf.owner_id} hasAvatar={shelf.owner_has_avatar} size={16} alt="" />
							{t('public.by', { owner: shelf.owner })} · {count(shelf)}
							{#if shelf.restricted}
								<span class="restricted"><Users size={12} strokeWidth={1.75} /> {t('public.restricted')}</span>
							{/if}
						</span>
						{#if shelf.description}
							<span class="description">{plainSnippet(shelf.description)}</span>
						{/if}
					</div>
					{#if shelf.cover_books.length > 0}
						<div class="glimpse" aria-hidden="true">
							{#each shelf.cover_books as id (id)}
								<img src={`/api/public/books/${id}/cover?size=thumb`} alt="" loading="lazy" />
							{/each}
						</div>
					{/if}
				</a>
			</li>
		{/each}
	</ul>
{:else}
	<div class="grid">
		{#each data.publicShelves as shelf (shelf.id)}
			<a class="card" href={shelfHref(shelf)}>
				<div class="cover">
					<img src={shelfCover(shelf)} alt="" loading="lazy" />
				</div>
				<div class="meta">
					<span class="name">{shelf.name}</span>
					<span class="owner">
						<Avatar userId={shelf.owner_id} hasAvatar={shelf.owner_has_avatar} size={18} alt="" />
						{t('public.by', { owner: shelf.owner })} · {shelf.book_count}
					</span>
					{#if shelf.restricted}
						<span class="restricted"><Users size={12} strokeWidth={1.75} /> {t('public.restricted')}</span>
					{/if}
				</div>
			</a>
		{/each}
	</div>
{/if}

<style>
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		margin: 0 0 1.5rem;
	}
	h1 {
		font-size: 1.4rem;
		margin: 0;
	}
	.empty {
		color: var(--muted);
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
		gap: 1.25rem;
	}
	.card {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
		background: var(--card);
		color: var(--fg);
		box-shadow: var(--shadow);
	}
	.card:hover {
		text-decoration: none;
		border-color: var(--accent);
	}
	.cover {
		height: 7rem;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg);
		color: var(--gold);
		border-bottom: 1px solid var(--border);
	}
	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.meta {
		display: flex;
		flex-direction: column;
		padding: 0.6rem 0.8rem;
	}
	.name {
		font-weight: 600;
		font-size: 0.95rem;
	}
	.owner {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.8rem;
		color: var(--muted);
	}
	.restricted {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.75rem;
		color: var(--muted);
	}

	/* The list: one shelf per row, with its description and a glimpse of its books. */
	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.rows li {
		border-top: 1px solid var(--border);
	}
	.rows li:last-child {
		border-bottom: 1px solid var(--border);
	}
	.row {
		display: grid;
		grid-template-columns: 4.5rem minmax(0, 1fr) auto;
		gap: 1rem;
		align-items: center;
		padding: 0.75rem 0.25rem;
		color: var(--fg);
	}
	.row:hover {
		text-decoration: none;
		background: var(--card);
	}
	.row:hover .name {
		color: var(--accent);
	}
	.thumb {
		width: 4.5rem;
		height: 3rem;
		object-fit: cover;
		border-radius: 4px;
		border: 1px solid var(--border);
		background: var(--bg);
	}
	.info {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.description {
		margin-top: 0.25rem;
		font-size: 0.85rem;
		color: var(--muted);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	/* The covers lie fanned out, each a little over the one before. */
	.glimpse {
		display: flex;
		padding-right: 0.25rem;
	}
	.glimpse img {
		width: 2.4rem;
		aspect-ratio: 2 / 3;
		object-fit: cover;
		border-radius: 2px 3px 3px 2px;
		border: 1px solid var(--border);
		background: var(--card);
		box-shadow: -2px 2px 6px rgb(0 0 0 / 0.25);
	}
	.glimpse img + img {
		margin-left: -1rem;
	}
	@media (max-width: 36rem) {
		.row {
			grid-template-columns: 4.5rem minmax(0, 1fr);
		}
		.glimpse {
			grid-column: 2;
		}
	}
</style>
