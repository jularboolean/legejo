<script lang="ts">
	import { ArrowDown, ArrowUp, ArrowUpDown, Check } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import type { MessageKey } from '#lib/i18n';
	import { defaultDir, type SortDir, type SortKey } from '#lib/library';
	import Popover from './Popover.svelte';

	let {
		keys,
		sort,
		dir,
		onchange
	}: {
		/** What the list can be ordered by, in menu order. */
		keys: SortKey[];
		sort: SortKey;
		dir: SortDir;
		onchange: (sort: SortKey, dir: SortDir) => void;
	} = $props();

	const sortLabel = (key: SortKey) => t(`library.sort.${key}` as MessageKey);

	function choose(key: SortKey, close: () => void) {
		onchange(key, defaultDir(key));
		close();
	}
</script>

<div class="sort">
	<Popover label={t('library.sort')}>
		{#snippet button()}
			<ArrowUpDown size={13} />
			<span class="sort-label">{sortLabel(sort)}</span>
		{/snippet}
		{#snippet children(close)}
			<div role="menu" class="menu">
				{#each keys as key (key)}
					<button
						type="button"
						role="menuitemradio"
						aria-checked={sort === key}
						class="item"
						onclick={() => choose(key, close)}
					>
						<span class="check">{#if sort === key}<Check size={13} />{/if}</span>
						{sortLabel(key)}
					</button>
				{/each}
			</div>
		{/snippet}
	</Popover>
	<button
		type="button"
		class="ghost icon"
		title={dir === 'asc' ? t('library.sort.asc') : t('library.sort.desc')}
		aria-label={dir === 'asc' ? t('library.sort.asc') : t('library.sort.desc')}
		onclick={() => onchange(sort, dir === 'asc' ? 'desc' : 'asc')}
	>
		{#if dir === 'asc'}<ArrowUp size={14} />{:else}<ArrowDown size={14} />{/if}
	</button>
</div>

<style>
	.sort {
		display: flex;
		align-items: center;
		gap: 0.25rem;
	}
	.icon {
		padding: 0.36rem;
		color: var(--muted);
	}
	.menu {
		display: flex;
		flex-direction: column;
	}
	.item {
		justify-content: flex-start;
		gap: 0.5rem;
		background: none;
		color: var(--fg);
		font-size: 0.86rem;
		font-weight: 500;
		letter-spacing: 0;
		padding: 0.4rem 0.6rem;
		border-radius: 6px;
		text-align: left;
	}
	.item:hover {
		background: var(--bg);
		filter: none;
	}
	.item[aria-checked='true'] {
		color: var(--accent);
		font-weight: 600;
	}
	.check {
		display: inline-flex;
		width: 13px;
	}
	@media (max-width: 640px) {
		.sort-label {
			max-width: 8rem;
			overflow: hidden;
			text-overflow: ellipsis;
		}
	}
</style>
