<script lang="ts">
	// Above the library: books that have come onto shelves the user follows
	// since they last looked. Nothing at all when there are none.
	import { BellDot } from '@lucide/svelte';
	import { followNews, markAllSeen, shelfHref } from '#lib/follows.svelte';
	import { t } from '#lib/i18n';
</script>

{#if followNews.shelves.length > 0}
	<section class="news" aria-label={t('follow.newsTitle')}>
		<h2><BellDot size={14} /> {t('follow.newsTitle')}</h2>
		<ul>
			{#each followNews.shelves as shelf (shelf.id)}
				<li>
					<a class="shelf" href={shelfHref(shelf.owner, shelf.name)}>{shelf.name}</a>
					<span class="owner">{t('public.by', { owner: shelf.owner })}</span>
					<span class="count">{t('follow.newCount', { count: shelf.new_books })}</span>
					<span class="titles">
						{shelf.titles.map((b) => b.title).join(', ')}{#if shelf.new_books > shelf.titles.length}{' '}{t(
								'follow.more',
								{ count: shelf.new_books - shelf.titles.length }
							)}{/if}
					</span>
				</li>
			{/each}
		</ul>
		<button type="button" class="link" onclick={markAllSeen}>{t('follow.seen')}</button>
	</section>
{/if}

<style>
	.news {
		margin: 0 0 1.25rem;
		padding: 0.75rem 1rem;
		border: 1px solid var(--border);
		border-left: 3px solid var(--gold);
		border-radius: 8px;
		background: var(--card);
		font-size: 0.88rem;
	}
	h2 {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		margin: 0 0 0.4rem;
		font-size: 0.9rem;
		font-weight: 600;
	}
	ul {
		list-style: none;
		margin: 0 0 0.4rem;
		padding: 0;
		display: grid;
		gap: 0.25rem;
	}
	li {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0 0.5rem;
		min-width: 0;
	}
	.shelf {
		font-weight: 600;
	}
	.owner,
	.titles {
		color: var(--muted);
	}
	.titles {
		flex-basis: 100%;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.count {
		font-size: 0.75rem;
		color: var(--accent);
		border: 1px solid var(--accent);
		border-radius: 99px;
		padding: 0 0.45rem;
	}
	.link {
		background: none;
		border: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		font-size: 0.82rem;
		cursor: pointer;
	}
	.link:hover {
		text-decoration: underline;
		filter: none;
	}
</style>
