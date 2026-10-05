<script lang="ts">
	// A modal panel: slides in from a side on wide screens, from the bottom on
	// phones; `center` is a small dialog in the middle. Built on <dialog>, which
	// gives focus trapping and Escape for free.
	import type { Snippet } from 'svelte';
	import { X } from '@lucide/svelte';
	import { t } from '#lib/i18n';

	type Props = {
		open: boolean;
		title: string;
		side?: 'left' | 'right' | 'center';
		/**
		 * Leave the page behind the panel undimmed and keep a phone's bottom sheet
		 * low, so the effect of what is changed in the panel shows right away.
		 */
		preview?: boolean;
		onclose: () => void;
		/** Extra controls for the header, left of the close button. */
		actions?: Snippet;
		children: Snippet;
	};
	let { open, title, side = 'right', preview = false, onclose, actions, children }: Props = $props();

	let dialog: HTMLDialogElement;

	$effect(() => {
		if (open && !dialog.open) dialog.showModal();
		else if (!open && dialog.open) dialog.close();
	});
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog
	bind:this={dialog}
	class="sheet {side}"
	class:preview
	aria-label={title}
	{onclose}
	onclick={(e) => {
		// A click on the dialog element itself is a click on the backdrop.
		if (e.target === dialog) onclose();
	}}
>
	<div class="inner">
		<header>
			<h2>{title}</h2>
			{@render actions?.()}
			<button class="r-icon" onclick={onclose} aria-label={t('reader.close')} title={t('reader.close')}>
				<X size={20} />
			</button>
		</header>
		<div class="body">
			{@render children()}
		</div>
	</div>
</dialog>

<style>
	.sheet {
		position: fixed;
		inset: 0 auto 0 auto;
		margin: 0;
		padding: 0;
		width: min(24rem, 88vw);
		height: 100%;
		max-height: none;
		max-width: none;
		border: none;
		background: var(--r-surface);
		color: var(--r-fg);
		box-shadow: 0 0 40px rgba(0, 0, 0, 0.22);
		overflow: hidden;
	}
	.sheet.left {
		left: 0;
		border-right: 1px solid var(--r-border);
		padding-left: env(safe-area-inset-left);
		width: calc(min(24rem, 88vw) + env(safe-area-inset-left));
	}
	.sheet.right {
		right: 0;
		border-left: 1px solid var(--r-border);
		padding-right: env(safe-area-inset-right);
		width: calc(min(24rem, 88vw) + env(safe-area-inset-right));
	}
	.sheet.center {
		inset: 0;
		margin: auto;
		width: min(30rem, calc(100vw - 2rem));
		height: fit-content;
		max-height: min(40rem, calc(100dvh - 2rem));
		border: 1px solid var(--r-border);
		border-radius: 16px;
	}
	.sheet::backdrop {
		background: rgba(0, 0, 0, 0.35);
	}
	.sheet.preview::backdrop {
		background: transparent;
	}
	.sheet[open] {
		animation: slide-in 0.2s ease-out;
	}
	.sheet.left[open] {
		--from: translateX(-100%);
	}
	.sheet.right[open] {
		--from: translateX(100%);
	}
	.sheet.center[open] {
		animation: pop-in 0.15s ease-out;
	}
	@keyframes slide-in {
		from {
			transform: var(--from);
		}
	}
	@keyframes pop-in {
		from {
			opacity: 0;
			transform: scale(0.97);
		}
	}
	.inner {
		display: flex;
		flex-direction: column;
		height: 100%;
		max-height: inherit;
	}
	header {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.35rem 0.4rem 0.35rem 1.25rem;
		border-bottom: 1px solid var(--r-border);
		flex-shrink: 0;
	}
	h2 {
		flex: 1;
		min-width: 0;
		margin: 0;
		font-size: 1.05rem;
	}
	.body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		overscroll-behavior: contain;
		-webkit-overflow-scrolling: touch;
		padding-bottom: env(safe-area-inset-bottom);
	}

	/* Phones in portrait: a bottom sheet, within reach of the thumb. */
	@media (max-width: 40rem) {
		.sheet.left,
		.sheet.right {
			inset: auto 0 0 0;
			width: 100%;
			height: auto;
			max-height: 85dvh;
			padding: 0;
			border: none;
			border-top: 1px solid var(--r-border);
			border-radius: 16px 16px 0 0;
		}
		.sheet.preview {
			max-height: 56dvh;
		}
		.sheet.left[open],
		.sheet.right[open] {
			--from: translateY(100%);
		}
		.inner {
			height: auto;
		}
	}
</style>
