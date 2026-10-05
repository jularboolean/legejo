<script lang="ts">
	import { tick } from 'svelte';
	import { LoaderCircle, Search, X } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import Sheet from './Sheet.svelte';
	import type { SearchHit } from './types';

	type Props = {
		open: boolean;
		/** The query the results belong to; empty before the first search. */
		query: string;
		hits: SearchHit[];
		searching: boolean;
		/** Share of the book searched so far, 0–1. */
		searched: number;
		/** The search stopped at the cap on the number of hits. */
		truncated: boolean;
		/** Hit last jumped to. */
		currentCfi: string | null;
		onsearch: (query: string) => void;
		onselect: (hit: SearchHit) => void;
		onclose: () => void;
	};
	let { open, query, hits, searching, searched, truncated, currentCfi, onsearch, onselect, onclose }: Props =
		$props();

	let input = $state<HTMLInputElement | null>(null);
	let draft = $state('');

	$effect(() => {
		if (!open) return;
		tick().then(() => {
			input?.focus();
			input?.select();
		});
	});

	function submit(event: SubmitEvent) {
		event.preventDefault();
		onsearch(draft.trim());
	}

	function clear() {
		draft = '';
		onsearch('');
		input?.focus();
	}

	const summary = $derived(
		!query
			? ''
			: searching
				? t('reader.search.searching', { percent: Math.round(searched * 100) })
				: hits.length === 0
					? t('reader.search.none', { query })
					: truncated
						? t('reader.search.truncated', { count: hits.length })
						: hits.length === 1
							? t('reader.search.one')
							: t('reader.search.count', { count: hits.length })
	);
</script>

<Sheet {open} title={t('reader.search')} side="right" {onclose}>
	<div class="search">
		<form onsubmit={submit} role="search">
			<span class="glass" aria-hidden="true"><Search size={17} /></span>
			<!-- 16px text, or iOS zooms the page in when the field gets focus. -->
			<input
				bind:this={input}
				bind:value={draft}
				type="search"
				enterkeyhint="search"
				autocomplete="off"
				autocapitalize="off"
				spellcheck="false"
				placeholder={t('reader.search.placeholder')}
				aria-label={t('reader.search.placeholder')}
			/>
			{#if draft}
				<button type="button" class="r-icon clear" onclick={clear} aria-label={t('reader.search.clear')}>
					<X size={16} />
				</button>
			{/if}
		</form>

		<p class="summary" role="status">
			{#if searching}<span class="spinner"><LoaderCircle size={14} /></span>{/if}
			{summary}
		</p>

		{#if hits.length > 0}
			<ol>
				{#each hits as hit (hit.rangeCfi)}
					<li>
						<button
							class="hit"
							class:current={hit.rangeCfi === currentCfi}
							onclick={() => onselect(hit)}
						>
							{#if hit.chapterLabel}<span class="chapter">{hit.chapterLabel}</span>{/if}
							<span class="excerpt">…{hit.before}<mark>{hit.match}</mark>{hit.after}…</span>
						</button>
					</li>
				{/each}
			</ol>
		{:else if !query}
			<p class="hint">{t('reader.search.hint')}</p>
		{/if}
	</div>
</Sheet>

<style>
	.search {
		display: flex;
		flex-direction: column;
		min-height: 12rem;
	}
	form {
		position: sticky;
		top: 0;
		z-index: 1;
		display: flex;
		align-items: center;
		margin: 0;
		padding: 0.75rem 1rem 0.5rem;
		background: var(--r-surface);
	}
	.glass {
		position: absolute;
		left: 1.7rem;
		display: inline-flex;
		color: var(--r-muted);
		pointer-events: none;
	}
	input {
		flex: 1;
		min-width: 0;
		height: 2.75rem;
		padding: 0 2.75rem 0 2.4rem;
		border: 1px solid var(--r-border);
		border-radius: 12px;
		background: var(--r-bg);
		color: var(--r-fg);
		font-size: 16px;
		-webkit-appearance: none;
		appearance: none;
	}
	input::-webkit-search-cancel-button {
		display: none;
	}
	input:focus {
		outline: 2px solid var(--r-link);
		outline-offset: 0;
	}
	.clear {
		position: absolute;
		right: 1rem;
	}
	.summary {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		min-height: 1.4rem;
		margin: 0;
		padding: 0 1.25rem 0.5rem;
		font-size: 0.8rem;
		color: var(--r-muted);
	}
	.hint {
		margin: 0;
		padding: 0.5rem 1.25rem 1.5rem;
		color: var(--r-muted);
	}
	ol {
		list-style: none;
		margin: 0;
		padding: 0 0 0.5rem;
	}
	li {
		border-top: 1px solid color-mix(in srgb, var(--r-border) 60%, transparent);
	}
	.hit {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 0.15rem;
		width: 100%;
		min-height: 2.75rem;
		padding: 0.6rem 1.25rem;
		border: none;
		border-left: 3px solid transparent;
		border-radius: 0;
		text-align: left;
		font-weight: 400;
		-webkit-user-select: text;
		user-select: text;
	}
	.hit:focus-visible {
		outline-offset: -2px;
	}
	@media (hover: hover) {
		.hit:hover {
			background: color-mix(in srgb, var(--r-fg) 7%, transparent);
		}
	}
	.hit.current {
		border-left-color: var(--r-link);
		background: color-mix(in srgb, var(--r-link) 10%, transparent);
	}
	.chapter {
		font-size: 0.72rem;
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--r-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.excerpt {
		font-family: Georgia, 'Times New Roman', serif;
		font-size: 0.95rem;
		line-height: 1.45;
		overflow-wrap: anywhere;
	}
	mark {
		padding: 0 0.1em;
		border-radius: 3px;
		background: color-mix(in srgb, var(--r-link) 30%, transparent);
		color: inherit;
		font-weight: 600;
	}
	.spinner {
		display: inline-flex;
		animation: spin 1s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
