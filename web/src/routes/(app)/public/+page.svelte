<script lang="ts">
	import { Users } from '@lucide/svelte';
	import Avatar from '#lib/Avatar.svelte';
	import { defaultShelfCover } from '#lib/shelfCovers';
	import { t } from '#lib/i18n';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();
</script>

<h1>{t('public.heading')}</h1>

{#if data.publicShelves.length === 0}
	<p class="empty">{t('public.empty')}</p>
{:else}
	<div class="grid">
		{#each data.publicShelves as shelf (shelf.id)}
			<a class="card" href={`/public/${encodeURIComponent(shelf.owner)}/${encodeURIComponent(shelf.name)}`}>
				<div class="cover">
					<img
						src={shelf.has_cover ? `/api/public/shelves/${shelf.id}/cover` : defaultShelfCover(shelf.id)}
						alt=""
						loading="lazy"
					/>
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
	h1 {
		font-size: 1.4rem;
		margin: 0 0 1.5rem;
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
</style>
