<script lang="ts">
	import { t } from '#lib/i18n';
	import Dove from '#lib/Dove.svelte';

	let {
		value,
		onchange,
		size = 26
	}: {
		value: number | null;
		/** Without it the rating is shown read-only. Null clears the rating. */
		onchange?: (value: number | null) => void;
		size?: number;
	} = $props();

	const STEPS = [1, 2, 3, 4, 5];
	let hover = $state<number | null>(null);
	const shown = $derived(hover ?? value ?? 0);

	function keys(e: KeyboardEvent) {
		if (!onchange) return;
		const current = value ?? 0;
		if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
			e.preventDefault();
			onchange(Math.min(5, current + 1));
		} else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
			e.preventDefault();
			onchange(current <= 1 ? null : current - 1);
		}
	}
</script>

{#if onchange}
	<div
		class="rating editable"
		role="radiogroup"
		tabindex="-1"
		aria-label={t('book.rating')}
		onmouseleave={() => (hover = null)}
		onkeydown={keys}
	>
		{#each STEPS as step (step)}
			<!-- Picking the current rating again clears it. -->
			<button
				type="button"
				role="radio"
				aria-checked={value === step}
				aria-label={t('book.rating.value', { n: step })}
				title={value === step ? t('book.rating.clear') : t('book.rating.value', { n: step })}
				tabindex={step === (value ?? 1) ? 0 : -1}
				onmouseenter={() => (hover = step)}
				onclick={() => onchange(value === step ? null : step)}
			>
				<Dove {size} filled={step <= shown} />
			</button>
		{/each}
	</div>
{:else if value}
	<span class="rating" role="img" aria-label={t('book.rating.value', { n: value })} title={t('book.rating.value', { n: value })}>
		{#each STEPS as step (step)}
			<span class="dove"><Dove {size} filled={step <= value} /></span>
		{/each}
	</span>
{/if}

<style>
	.rating {
		display: inline-flex;
		align-items: center;
		gap: 0.15rem;
		color: var(--muted);
	}
	.dove {
		display: inline-flex;
	}
	button {
		background: none;
		border: none;
		border-radius: 6px;
		padding: 0.15rem;
		color: inherit;
		transition: transform 0.1s;
	}
	button:hover {
		filter: none;
		transform: translateY(-1px) scale(1.08);
	}
	button:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}
</style>
