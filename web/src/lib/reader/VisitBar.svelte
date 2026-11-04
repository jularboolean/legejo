<script lang="ts">
	import { BookmarkCheck, CornerUpLeft, Eye } from '@lucide/svelte';
	import { onDestroy } from 'svelte';
	import { t } from '#lib/i18n';

	type Props = {
		/** The book is open as a visit: the reading position is not saved. */
		visiting: boolean;
		/** Shown with the toolbars; the notice that the visit is over shows either way. */
		visible: boolean;
		/** Distance from the top of the reader, below the toolbar. */
		top: string;
		/** Where the reading position is, as a percentage; null when there is none, or the book is finished. */
		kept: number | null;
		/** Read on from here: the position is saved from now on. */
		onread: () => void;
		/** Go to the reading position; null when there is none. */
		onplace: (() => void) | null;
	};
	let { visiting, visible, top, kept, onread, onplace }: Props = $props();

	const NOTICE_MS = 2200;
	let notice = $state(false);
	let timer: ReturnType<typeof setTimeout> | null = null;
	// svelte-ignore state_referenced_locally
	let was = visiting;

	// Once the visit is over, however it ended, say so for a moment.
	$effect(() => {
		if (was && !visiting) {
			notice = true;
			if (timer) clearTimeout(timer);
			timer = setTimeout(() => (notice = false), NOTICE_MS);
		}
		was = visiting;
	});

	onDestroy(() => {
		if (timer) clearTimeout(timer);
	});

	const line = $derived(kept === null ? t('reader.visit.none') : t('reader.visit.kept', { percent: t('reader.percent', { percent: Math.round(kept * 100) }) }));
</script>

{#if visiting}
	<div class="visit" class:hidden={!visible} inert={!visible} style:top role="status">
		<Eye size={15} aria-hidden="true" />
		<span class="line"><strong>{t('reader.visit')}</strong> · {line}</span>
		<span class="actions">
			{#if onplace}
				<button type="button" onclick={onplace}>
					<CornerUpLeft size={14} />
					{t('reader.visit.toPlace')}
				</button>
			{/if}
			<button type="button" class="main" onclick={onread}>
				<BookmarkCheck size={14} />
				{t('reader.visit.readHere')}
			</button>
		</span>
	</div>
{:else if notice}
	<div class="visit notice" style:top role="status">
		<BookmarkCheck size={15} aria-hidden="true" />
		<span class="line">{t('reader.visit.saving')}</span>
	</div>
{/if}

<style>
	.visit {
		position: absolute;
		z-index: 3;
		left: 50%;
		translate: -50% 0;
		margin-top: 0.5rem;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: center;
		gap: 0.35rem 0.6rem;
		width: max-content;
		max-width: calc(100% - 1.5rem);
		padding: 0.4rem 0.5rem 0.4rem 0.85rem;
		border-radius: 1.2rem;
		background: var(--r-surface);
		color: var(--r-fg);
		border: 1px solid var(--r-border);
		box-shadow: 0 2px 14px rgba(0, 0, 0, 0.16);
		font-size: 0.82rem;
		transition:
			opacity 0.2s ease,
			transform 0.2s ease;
	}
	.visit.hidden {
		opacity: 0;
		transform: translateY(-0.5rem);
		pointer-events: none;
	}
	.visit.notice {
		padding-right: 0.85rem;
		pointer-events: none;
	}
	.line {
		color: var(--r-muted);
	}
	.line strong {
		color: var(--r-fg);
		font-weight: 600;
	}
	.actions {
		display: flex;
		gap: 0.3rem;
	}
	button {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		min-height: 2rem;
		padding: 0 0.7rem;
		border-radius: 99px;
		border: 1px solid var(--r-border);
		background: transparent;
		color: var(--r-fg);
		font: inherit;
		cursor: pointer;
		white-space: nowrap;
	}
	button.main {
		background: var(--r-fg);
		border-color: var(--r-fg);
		color: var(--r-surface);
	}
	@media (prefers-reduced-motion: reduce) {
		.visit {
			transition: none;
		}
	}
</style>
