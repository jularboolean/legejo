<script lang="ts">
	import { tick } from 'svelte';
	import { t } from '#lib/i18n';
	import Sheet from './Sheet.svelte';
	import type { TocItem } from './types';

	type Props = {
		open: boolean;
		toc: TocItem[];
		/** Id of the entry being read. */
		currentId: string | null;
		onselect: (item: TocItem) => void;
		onclose: () => void;
	};
	let { open, toc, currentId, onselect, onclose }: Props = $props();

	let list = $state<HTMLElement | null>(null);

	// Bring the current chapter into view each time the panel opens.
	$effect(() => {
		if (!open) return;
		tick().then(() => {
			list?.querySelector('[aria-current="true"]')?.scrollIntoView({ block: 'center' });
		});
	});
</script>

{#snippet entries(items: TocItem[])}
	<ol>
		{#each items as item (item.id)}
			<li>
				<button
					class="entry"
					class:current={item.id === currentId}
					aria-current={item.id === currentId ? 'true' : undefined}
					style:padding-left={`${1.25 + item.depth * 1.1}rem`}
					onclick={() => onselect(item)}
				>
					{item.label}
				</button>
				{#if item.children.length > 0}
					{@render entries(item.children)}
				{/if}
			</li>
		{/each}
	</ol>
{/snippet}

<Sheet {open} title={t('reader.toc')} side="left" {onclose}>
	{#if toc.length === 0}
		<p class="empty">{t('reader.tocEmpty')}</p>
	{:else}
		<nav bind:this={list} aria-label={t('reader.toc')}>
			{@render entries(toc)}
		</nav>
	{/if}
</Sheet>

<style>
	nav {
		padding: 0.4rem 0;
	}
	ol {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.entry {
		display: block;
		width: 100%;
		text-align: left;
		min-height: 2.75rem;
		padding: 0.7rem 1.25rem;
		border-radius: 0;
		border: none;
		border-left: 3px solid transparent;
		color: var(--r-fg);
		line-height: 1.3;
	}
	.entry:focus-visible {
		outline-offset: -2px;
	}
	@media (hover: hover) {
		.entry:hover {
			background: color-mix(in srgb, var(--r-fg) 7%, transparent);
		}
	}
	.entry.current {
		border-left-color: var(--r-link);
		color: var(--r-link);
		font-weight: 600;
		background: color-mix(in srgb, var(--r-link) 10%, transparent);
	}
	.empty {
		padding: 1rem 1.25rem;
		color: var(--r-muted);
	}
</style>
