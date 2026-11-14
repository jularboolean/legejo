<script lang="ts">
	import { Layers, LayoutGrid, List } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import type { ViewMode } from '#lib/library';

	// With `onseries` (My library only) a third button groups the books by
	// series; while it is on, neither of the other two is pressed.
	let {
		view,
		onchange,
		series = false,
		onseries
	}: { view: ViewMode; onchange: (view: ViewMode) => void; series?: boolean; onseries?: () => void } = $props();
</script>

<div class="seg" role="group" aria-label={t('library.view')}>
	<button
		type="button"
		aria-pressed={!series && view === 'grid'}
		title={t('library.view.grid')}
		aria-label={t('library.view.grid')}
		onclick={() => onchange('grid')}
	>
		<LayoutGrid size={14} />
	</button>
	<button
		type="button"
		aria-pressed={!series && view === 'list'}
		title={t('library.view.list')}
		aria-label={t('library.view.list')}
		onclick={() => onchange('list')}
	>
		<List size={14} />
	</button>
	{#if onseries}
		<button
			type="button"
			aria-pressed={series}
			title={t('library.view.series')}
			aria-label={t('library.view.series')}
			onclick={onseries}
		>
			<Layers size={14} />
		</button>
	{/if}
</div>

<style>
	.seg {
		display: inline-flex;
		flex-shrink: 0;
		border: 1px solid var(--border);
		border-radius: 6px;
		overflow: hidden;
	}
	.seg button {
		background: none;
		color: var(--muted);
		border: none;
		border-radius: 0;
		padding: 0.36rem 0.5rem;
	}
	.seg button + button {
		border-left: 1px solid var(--border);
	}
	.seg button:hover {
		color: var(--fg);
		filter: none;
	}
	.seg button[aria-pressed='true'] {
		background: var(--accent);
		color: var(--bg);
	}
</style>
