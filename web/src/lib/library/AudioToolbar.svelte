<script lang="ts">
	import { Search, SlidersHorizontal, X } from '@lucide/svelte';
	import { emptyAudioFilters, type AudioFilters } from '#lib/audio';
	import { t } from '#lib/i18n';
	import type { ViewMode } from '#lib/library';
	import Popover from './Popover.svelte';
	import ViewSwitch from './ViewSwitch.svelte';

	type Option = { key: string; label: string; count: number };
	type Field = 'category' | 'tag' | 'language';

	let {
		filters = $bindable(),
		view = $bindable(),
		categories,
		tags,
		languages
	}: {
		filters: AudioFilters;
		view: ViewMode;
		/** The values in use among the audiobooks, most common first. */
		categories: Option[];
		tags: Option[];
		languages: Option[];
	} = $props();

	const groups = $derived(
		(
			[
				{ field: 'category', heading: t('book.category'), options: categories },
				{ field: 'tag', heading: t('book.tags'), options: tags },
				{ field: 'language', heading: t('library.filter.language'), options: languages }
			] as { field: Field; heading: string; options: Option[] }[]
		).filter((group) => group.options.length > 0)
	);

	// Picking the active value again clears that filter.
	function toggle(field: Field, key: string) {
		filters = { ...filters, [field]: filters[field] === key ? null : key };
	}

	const active = $derived(
		groups.flatMap((group) => {
			const key = filters[group.field];
			if (!key) return [];
			return [{ field: group.field, label: group.options.find((o) => o.key === key)?.label ?? key }];
		})
	);
</script>

<div class="bar">
	<div class="text">
		<Search size={14} class="text-icon" aria-hidden="true" />
		<input
			type="search"
			placeholder={t('audio.filterPlaceholder')}
			aria-label={t('audio.filterPlaceholder')}
			value={filters.text}
			oninput={(e) => (filters = { ...filters, text: e.currentTarget.value })}
		/>
	</div>

	<div class="controls">
		{#if groups.length > 0}
			<Popover label={t('library.filter')} align="right">
				{#snippet button()}
					<SlidersHorizontal size={13} />
					{t('library.filter')}
					{#if active.length > 0}<span class="n">{active.length}</span>{/if}
				{/snippet}
				{#snippet children()}
					<div class="groups">
						{#each groups as group (group.field)}
							<section>
								<h3>{group.heading}</h3>
								<div class="options">
									{#each group.options as option (option.key)}
										<button
											type="button"
											class="option"
											aria-pressed={filters[group.field] === option.key}
											onclick={() => toggle(group.field, option.key)}
										>
											{option.label}
											<span class="num">{option.count}</span>
										</button>
									{/each}
								</div>
							</section>
						{/each}
					</div>
				{/snippet}
			</Popover>
		{/if}

		<ViewSwitch {view} onchange={(v) => (view = v)} />
	</div>
</div>

{#if active.length > 0}
	<div class="active">
		{#each active as chip (chip.field)}
			<button
				type="button"
				class="chip"
				aria-label={t('library.removeFilter', { name: chip.label })}
				onclick={() => (filters = { ...filters, [chip.field]: null })}
			>
				{chip.label}
				<X size={12} />
			</button>
		{/each}
		<button type="button" class="clear" onclick={() => (filters = { ...emptyAudioFilters(), text: filters.text })}>
			{t('library.clear')}
		</button>
	</div>
{/if}

<style>
	.bar {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}
	.text {
		position: relative;
		flex: 1;
		min-width: 10rem;
		max-width: 26rem;
	}
	.text :global(.text-icon) {
		position: absolute;
		left: 0.7rem;
		top: 50%;
		transform: translateY(-50%);
		color: var(--muted);
		pointer-events: none;
	}
	.text input {
		padding: 0.38rem 0.7rem 0.38rem 2rem;
		font-size: 0.88rem;
		border-radius: 8px;
	}
	.controls {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-left: auto;
	}
	.n {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 1.15rem;
		height: 1.15rem;
		padding: 0 0.3rem;
		border-radius: 99px;
		background: var(--accent);
		color: var(--bg);
		font-size: 0.7rem;
		font-weight: 700;
	}
	.groups {
		display: flex;
		flex-direction: column;
		gap: 0.9rem;
		padding: 0.5rem 0.45rem 0.55rem;
		width: min(22rem, 78vw);
	}
	h3 {
		margin: 0 0 0.4rem;
		font-family: inherit;
		font-size: 0.7rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--muted);
	}
	.options {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
	}
	.option {
		background: none;
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 99px;
		font-size: 0.8rem;
		font-weight: 500;
		letter-spacing: 0;
		padding: 0.2rem 0.65rem;
	}
	.option:hover {
		border-color: var(--accent);
		filter: none;
	}
	.option[aria-pressed='true'] {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--bg);
	}
	.num {
		font-size: 0.72rem;
		font-variant-numeric: tabular-nums;
		opacity: 0.6;
	}
	.active {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
		margin: -0.25rem 0 1rem;
	}
	.chip {
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		color: var(--accent);
		border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
		border-radius: 99px;
		font-size: 0.8rem;
		font-weight: 600;
		letter-spacing: 0;
		padding: 0.15rem 0.5rem 0.15rem 0.7rem;
		gap: 0.3rem;
	}
	.chip:hover {
		background: color-mix(in srgb, var(--accent) 20%, transparent);
		filter: none;
	}
	.clear {
		background: none;
		color: var(--muted);
		font-size: 0.8rem;
		font-weight: 500;
		letter-spacing: 0;
		padding: 0.15rem 0.4rem;
	}
	.clear:hover {
		color: var(--fg);
		filter: none;
		text-decoration: underline;
	}
	@media (max-width: 40rem) {
		.bar {
			flex-wrap: wrap;
		}
		.text {
			flex-basis: 100%;
			max-width: none;
		}
		.controls {
			position: relative;
			margin-left: 0;
			width: 100%;
			justify-content: space-between;
		}
		/* A menu anchored to its own button runs off the screen here; anchor
		   it to the row instead, so it spans it edge to edge. */
		.bar .controls :global(.popover) {
			position: static;
		}
		.bar .controls :global(.popover .panel) {
			left: 0;
			right: 0;
		}
		.groups {
			width: auto;
		}
	}
</style>
