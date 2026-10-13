<script lang="ts">
	// What the app says about the network, so that a press on a link is never
	// met with silence: a line along the top while a page is being fetched, a
	// word when that takes long, and a word when there is no connection.
	import { navigating } from '$app/state';
	import { WifiOff } from '@lucide/svelte';
	import { t } from '#lib/i18n';

	/** A page that comes at once shows no line at all. */
	const SHOWN_AFTER_MS = 150;
	const SLOW_AFTER_MS = 5000;

	let fetching = $state(false);
	let slow = $state(false);
	let offline = $state(typeof navigator !== 'undefined' && navigator.onLine === false);

	$effect(() => {
		if (!navigating.to) {
			fetching = false;
			slow = false;
			return;
		}
		const shown = setTimeout(() => (fetching = true), SHOWN_AFTER_MS);
		const late = setTimeout(() => (slow = true), SLOW_AFTER_MS);
		return () => {
			clearTimeout(shown);
			clearTimeout(late);
		};
	});
</script>

<svelte:window ononline={() => (offline = false)} onoffline={() => (offline = true)} />

{#if fetching}
	<div class="line" role="progressbar" aria-label={t('net.fetching')}></div>
{/if}

{#if offline || slow}
	<div class="notice" role="status">
		<WifiOff size={15} />
		<span>{offline ? t('net.offline') : t('net.slow')}</span>
		<a href="/offline">{t('offline.toSaved')}</a>
	</div>
{/if}

<style>
	.line {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		z-index: 200;
		height: 3px;
		overflow: hidden;
		background: color-mix(in srgb, var(--accent) 25%, transparent);
		pointer-events: none;
	}
	/* This one moves whatever the design's setting for animations says: it
	   is how the app shows that it is working. */
	.line::before {
		content: '';
		position: absolute;
		inset: 0;
		width: 40%;
		background: var(--accent);
		animation: passing 1.1s ease-in-out infinite;
	}
	@keyframes passing {
		from {
			transform: translateX(-100%);
		}
		to {
			transform: translateX(250%);
		}
	}
	.notice {
		position: fixed;
		left: 50%;
		bottom: calc(1rem + env(safe-area-inset-bottom));
		transform: translateX(-50%);
		z-index: 200;
		display: flex;
		align-items: center;
		gap: 0.6rem;
		max-width: calc(100vw - 2rem);
		padding: 0.55rem 0.9rem;
		border-radius: 10px;
		background: var(--fg);
		color: var(--bg);
		font-size: 0.86rem;
		box-shadow: var(--shadow);
	}
	.notice a {
		color: inherit;
		font-weight: 600;
		text-decoration: underline;
		white-space: nowrap;
	}
</style>
