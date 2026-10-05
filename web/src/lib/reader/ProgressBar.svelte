<script lang="ts">
	import { ChevronLeft, ChevronRight, CornerUpLeft } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import type { ReaderLocation } from './types';

	type Props = {
		location: ReaderLocation | null;
		/** Positions are known, so the slider can be used. */
		ready: boolean;
		/** Show the controls; otherwise only the slim status line. */
		expanded: boolean;
		/** Whether pages are shown (as opposed to scrolling); adds the page counts. */
		paginated: boolean;
		/** Right-to-left book: the page buttons swap sides. */
		rtl: boolean;
		/** Chapter label at an overall position (0–1), for the preview while dragging. */
		chapterAt: (fraction: number) => string | null;
		/**
		 * The reader just jumped somewhere (contents, search, a link): offer the
		 * way back, in place of the chapter title.
		 */
		canReturn: boolean;
		onreturn: () => void;
		onseek: (fraction: number) => void;
		onprev: () => void;
		onnext: () => void;
	};
	let {
		location,
		ready,
		expanded,
		paginated,
		rtl,
		chapterAt,
		canReturn,
		onreturn,
		onseek,
		onprev,
		onnext
	}: Props = $props();

	const STEPS = 1000;

	/** Position being dragged to, before it is committed. */
	let scrub = $state<number | null>(null);

	const percent = $derived(scrub ?? location?.percent ?? 0);
	const percentText = $derived(t('reader.percent', { percent: Math.round(percent * 100) }));
	const chapter = $derived(
		scrub !== null ? (chapterAt(scrub) ?? '') : (location?.chapterLabel ?? '')
	);
	/** How much of the chapter is left; shown discreetly beside the percentage. */
	const remaining = $derived.by(() => {
		if (!paginated || !location || scrub !== null || location.atEnd) return '';
		if (location.pagesLeft === 0) return t('reader.pagesLeft.none');
		if (location.pagesLeft === 1) return t('reader.pagesLeft.one');
		return t('reader.pagesLeft', { count: location.pagesLeft });
	});

	function commit(value: number) {
		scrub = null;
		onseek(value / STEPS);
	}
</script>

{#snippet pageButton(forward: boolean)}
	{@const label = forward ? t('reader.nextPage') : t('reader.prevPage')}
	<button
		class="r-icon"
		onclick={forward ? onnext : onprev}
		disabled={!location || (forward ? location.atEnd : location.atStart)}
		aria-label={label}
		title={label}
	>
		{#if forward !== rtl}<ChevronRight size={22} />{:else}<ChevronLeft size={22} />{/if}
	</button>
{/snippet}

<footer class="progress" class:expanded>
	<div class="controls" inert={!expanded}>
		{@render pageButton(rtl)}
		<div class="track">
			<input
				type="range"
				min="0"
				max={STEPS}
				step="1"
				value={Math.round(percent * STEPS)}
				disabled={!ready}
				dir={rtl ? 'rtl' : 'ltr'}
				aria-label={t('reader.position')}
				aria-valuetext={percentText}
				oninput={(e) => (scrub = Number(e.currentTarget.value) / STEPS)}
				onchange={(e) => commit(Number(e.currentTarget.value))}
			/>
			<p class="hint">
				{#if !ready}
					{t('reader.calculating')}
				{:else if scrub !== null}
					{chapter ? `${chapter} · ${percentText}` : percentText}
				{:else if paginated && location}
					{t('reader.pageOf', { page: location.page, total: location.pagesInChapter })}
				{:else}
					&nbsp;
				{/if}
			</p>
		</div>
		{@render pageButton(!rtl)}
	</div>

	<!-- Overall progress as a hairline along the top edge of the status line. -->
	<div class="line" aria-hidden="true"><span style:width={`${percent * 100}%`}></span></div>

	<div class="status" aria-live="off">
		{#if canReturn && scrub === null}
			<button class="return" onclick={onreturn} aria-label={t('reader.return')} title={t('reader.return')}>
				<CornerUpLeft size={14} />
				{t('reader.return.short')}
			</button>
		{/if}
		<span class="chapter">{chapter}</span>
		<span class="numbers">
			{#if remaining}<span class="remaining">{remaining}</span>{/if}
			<span class="percent">{location ? percentText : ''}</span>
		</span>
	</div>
</footer>

<style>
	.progress {
		position: relative;
		z-index: 4;
		background: var(--r-bg);
		padding: 0 calc(1rem + env(safe-area-inset-right)) env(safe-area-inset-bottom)
			calc(1rem + env(safe-area-inset-left));
		transition: background 0.2s ease;
	}
	.progress.expanded {
		background: var(--r-surface);
	}
	/* The controls sit above the status line and overlap the page, so showing
	   them never changes the size of the page (which would repaginate). */
	.controls {
		position: absolute;
		inset: auto 0 100% 0;
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.35rem calc(0.35rem + env(safe-area-inset-right)) 0
			calc(0.35rem + env(safe-area-inset-left));
		background: color-mix(in srgb, var(--r-surface) 94%, transparent);
		-webkit-backdrop-filter: blur(12px);
		backdrop-filter: blur(12px);
		border-top: 1px solid var(--r-border);
		transition:
			transform 0.2s ease,
			opacity 0.2s ease;
	}
	.progress:not(.expanded) .controls {
		transform: translateY(1rem);
		opacity: 0;
		pointer-events: none;
	}
	.track {
		flex: 1;
		min-width: 0;
		padding-top: 0.35rem;
	}
	.hint {
		margin: 0;
		text-align: center;
		font-size: 0.75rem;
		color: var(--r-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.line {
		position: absolute;
		inset: 0 0 auto 0;
		height: 2px;
		background: color-mix(in srgb, var(--r-fg) 8%, transparent);
	}
	.line span {
		display: block;
		height: 100%;
		background: color-mix(in srgb, var(--r-link) 70%, transparent);
		transition: width 0.3s ease;
	}
	.status {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		height: 1.9rem;
		font-size: 0.76rem;
		color: var(--r-muted);
	}
	.chapter {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* Looks like part of the status line, but is a full-size touch target: it
	   reaches up into the page's bottom margin. */
	:global(.reader) .return {
		position: relative;
		gap: 0.3rem;
		flex-shrink: 0;
		height: 1.9rem;
		color: var(--r-link);
		font-size: inherit;
		font-weight: 600;
		animation: arrive 0.25s ease-out;
	}
	.return::before {
		content: '';
		position: absolute;
		inset: -0.85rem -0.5rem 0 -0.75rem;
	}
	@keyframes arrive {
		from {
			opacity: 0;
			transform: translateX(-0.4rem);
		}
	}
	.numbers {
		display: flex;
		gap: 0.75rem;
		flex-shrink: 0;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.remaining {
		opacity: 0.8;
	}
	.percent {
		min-width: 2.4rem;
		text-align: right;
	}
</style>
