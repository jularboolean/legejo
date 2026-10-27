<script lang="ts">
	import { ChevronDown } from '@lucide/svelte';
	import type { Snippet } from 'svelte';
	import { fade, rise } from '#lib/motion';

	let {
		label,
		align = 'left',
		up = false,
		button,
		children
	}: {
		/** Accessible name for the trigger. */
		label: string;
		align?: 'left' | 'right';
		/** Open above the trigger (for a bar at the bottom of the screen). */
		up?: boolean;
		button: Snippet;
		/** Receives a function that closes the panel. */
		children: Snippet<[() => void]>;
	} = $props();

	let open = $state(false);
	let root = $state<HTMLElement>();
	/** The room the panel has on screen, so a long one scrolls inside itself. */
	let room = $state<number | null>(null);

	function fit(panel: HTMLElement) {
		const measure = () => {
			const box = panel.getBoundingClientRect();
			// The tab bar of a phone covers the foot of the screen.
			const tabbar = document.querySelector('.tabbar')?.getBoundingClientRect();
			const floor = tabbar && tabbar.height > 0 ? Math.min(tabbar.top, window.innerHeight) : window.innerHeight;
			const space = up ? box.bottom : floor - box.top;
			room = Math.max(160, Math.floor(space - 12));
		};
		measure();
		window.addEventListener('resize', measure);
		return { destroy: () => window.removeEventListener('resize', measure) };
	}

	const close = () => (open = false);

	function onclick(e: MouseEvent) {
		if (open && root && !root.contains(e.target as Node)) close();
	}
	function onkeydown(e: KeyboardEvent) {
		if (open && e.key === 'Escape') {
			e.stopPropagation();
			close();
		}
	}
</script>

<svelte:window {onclick} {onkeydown} />

<div class="popover" bind:this={root}>
	<button
		type="button"
		class="ghost trigger"
		class:open
		aria-haspopup="true"
		aria-expanded={open}
		aria-label={label}
		onclick={() => (open = !open)}
	>
		{@render button()}
		<ChevronDown size={13} />
	</button>
	{#if open}
		<div class="panel" class:right={align === 'right'} class:up use:fit style:max-height={room ? `${room}px` : null} in:rise={{ y: up ? 10 : -10, duration: 180 }} out:fade={{ duration: 110 }}>
			{@render children(close)}
		</div>
	{/if}
</div>

<style>
	.popover {
		position: relative;
	}
	.trigger {
		white-space: nowrap;
		color: var(--fg);
		font-weight: 500;
		letter-spacing: 0;
	}
	.trigger.open {
		border-color: var(--accent);
	}
	.trigger :global(svg) {
		flex-shrink: 0;
	}
	.trigger :global(svg:last-child) {
		color: var(--muted);
		transition: transform 0.15s;
	}
	.trigger.open :global(svg:last-child) {
		transform: rotate(180deg);
	}
	.panel {
		position: absolute;
		top: calc(100% + 0.4rem);
		left: 0;
		z-index: 30;
		min-width: 13rem;
		background: var(--card);
		border: 1px solid var(--border);
		border-radius: 10px;
		box-shadow:
			var(--shadow),
			0 12px 32px rgba(20, 30, 34, 0.14);
		padding: 0.4rem;
		overflow-y: auto;
		overscroll-behavior: contain;
	}
	.panel.up {
		top: auto;
		bottom: calc(100% + 0.4rem);
	}
	.panel.right {
		left: auto;
		right: 0;
	}
</style>
