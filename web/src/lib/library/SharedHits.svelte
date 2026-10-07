<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { Check, Download } from '@lucide/svelte';
	import { getLocale, t } from '#lib/i18n';
	import { languageName, type ViewMode } from '#lib/library';
	import BookRows from '#lib/library/BookRows.svelte';
	import type { PublicHit } from '#lib/types';

	/** Books on shelves other users share, each with where it stands and a way to copy it. */
	let { hits, view }: { hits: PublicHit[]; view: ViewMode } = $props();

	let importing = $state<Set<number>>(new Set());
	let imported = $state<Set<number>>(new Set());
	let errorMsg = $state('');

	async function importBook(id: number) {
		importing = new Set([...importing, id]);
		errorMsg = '';
		try {
			const res = await fetch(`/api/public/books/${id}/import`, { method: 'POST' });
			if (res.ok) {
				imported = new Set([...imported, id]);
				await invalidateAll();
			} else {
				const body = await res.json().catch(() => null);
				errorMsg = body?.error ?? t('public.importFailed');
			}
		} catch {
			errorMsg = t('common.network');
		} finally {
			const next = new Set(importing);
			next.delete(id);
			importing = next;
		}
	}

	function publicBookPath(hit: PublicHit): string {
		return `/public/${encodeURIComponent(hit.owner)}/${encodeURIComponent(hit.shelf_name)}/${hit.uuid}`;
	}

	/** "in {shelf} by …" cut at the shelf's name, which is shown as a link. */
	function inShelf(hit: PublicHit): [string, string] {
		const mark = '\u0000';
		const text = t('search.inShelf', { shelf: mark, by: t('public.by', { owner: hit.owner }) });
		const at = text.indexOf(mark);
		return at < 0 ? [text, ''] : [text.slice(0, at), text.slice(at + 1)];
	}

	function publicCover(hit: PublicHit): string {
		return `/api/public/books/${hit.id}/cover?size=thumb&v=${encodeURIComponent(hit.updated_at ?? hit.created_at)}`;
	}
</script>

{#snippet hitContext(hit: PublicHit)}
	{@const [before, after] = inShelf(hit)}
	{before}<a
		class="shelfpill"
		href={`/public/${encodeURIComponent(hit.owner)}/${encodeURIComponent(hit.shelf_name)}`}>{hit.shelf_name}</a
	>{after}
{/snippet}

{#snippet hitAction(hit: PublicHit)}
	{#if hit.owned}
		<span class="done"><Check size={13} /> {t('public.owned')}</span>
	{:else if imported.has(hit.id)}
		<span class="done"><Check size={13} /> {t('public.imported')}</span>
	{:else}
		<button class="ghost import" disabled={importing.has(hit.id)} onclick={() => importBook(hit.id)}>
			<Download size={13} />
			{importing.has(hit.id) ? t('public.importing') : t('public.import')}
		</button>
	{/if}
{/snippet}

{#if errorMsg}<p class="error">{errorMsg}</p>{/if}

{#if view === 'list'}
	<BookRows
		books={hits}
		showLanguage
		href={publicBookPath}
		cover={publicCover}
		own={false}
		context={hitContext}
		action={hitAction}
	/>
{:else}
	<div class="grid">
		{#each hits as hit (hit.id)}
			<div class="card">
				<a class="cover" href={publicBookPath(hit)}>
					{#if hit.has_cover}
						<img src={publicCover(hit)} alt={hit.title} loading="lazy" />
					{:else}
						<span class="placeholder">{hit.title}</span>
					{/if}
				</a>
				<a class="title" href={publicBookPath(hit)} title={hit.title}>{hit.title}</a>
				{#if hit.author}<span class="sub">{hit.author}</span>{/if}
				{#if languageName(hit.language, getLocale())}
					<span class="sub">{languageName(hit.language, getLocale())}</span>
				{/if}
				<span class="context">{@render hitContext(hit)}</span>
				{@render hitAction(hit)}
			</div>
		{/each}
	</div>
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
		gap: 0.3rem;
		color: var(--fg);
	}
	.cover {
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
	.title {
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--fg);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	a.title:hover {
		color: var(--accent);
		text-decoration: none;
	}
	.sub {
		font-size: 0.8rem;
		color: var(--muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.context {
		font-size: 0.75rem;
		color: var(--gold);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.shelfpill {
		display: inline-block;
		padding: 0 0.5rem;
		border: 1px solid var(--gold);
		border-radius: 99px;
		color: var(--gold);
		font-weight: 600;
	}
	.shelfpill:hover {
		text-decoration: none;
		background: var(--card);
	}
	.import {
		font-size: 0.78rem;
		padding: 0.3rem 0.6rem;
		justify-content: center;
		margin-top: 0.15rem;
		white-space: nowrap;
	}
	.done {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.3rem;
		font-size: 0.8rem;
		color: var(--accent);
		padding: 0.3rem 0;
		white-space: nowrap;
	}
</style>
