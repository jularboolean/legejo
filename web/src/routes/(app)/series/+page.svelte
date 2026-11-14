<script lang="ts">
	import { Search } from '@lucide/svelte';
	import { getLocale, t } from '#lib/i18n';
	import { groupBySeries } from '#lib/library';
	import { fold } from '#lib/library/authors';
	import SeriesGrid from '#lib/library/SeriesGrid.svelte';
	import type { PageData } from './$types';

	// Every series in the library, by name; each card opens its series page.
	let { data }: { data: PageData } = $props();

	let query = $state('');

	const count = $derived(groupBySeries(data.books, getLocale()).series.length);
	const shown = $derived.by(() => {
		const q = fold(query.trim());
		return q ? data.books.filter((b) => fold(b.series!).includes(q)) : data.books;
	});
</script>

<header>
	<h1>{t('seriesIndex.heading')}</h1>
	<p class="summary">
		{t('seriesIndex.count', { count })} · {t('authors.books', { count: data.books.length })}
	</p>
</header>

{#if data.books.length === 0}
	<p class="empty">{t('seriesIndex.empty')}</p>
{:else}
	{#if count > 8}
		<div class="search">
			<Search size={14} class="search-icon" aria-hidden="true" />
			<input type="search" bind:value={query} placeholder={t('seriesIndex.search')} aria-label={t('seriesIndex.search')} />
		</div>
	{/if}
	{#if shown.length === 0}
		<p class="empty">{t('seriesIndex.noMatch')}</p>
	{:else}
		<SeriesGrid books={shown} loose={false} byName />
	{/if}
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
	.empty {
		color: var(--muted);
	}
	.search {
		position: relative;
		max-width: 22rem;
		margin-bottom: 1.25rem;
	}
	.search :global(.search-icon) {
		position: absolute;
		left: 0.7rem;
		top: 50%;
		transform: translateY(-50%);
		color: var(--muted);
	}
	.search input {
		padding-left: 2rem;
	}
</style>
