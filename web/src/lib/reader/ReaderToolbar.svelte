<script lang="ts">
	import { ALargeSmall, ArrowLeft, Keyboard, List, Maximize, Minimize, Search } from '@lucide/svelte';
	import { t } from '#lib/i18n';

	type Props = {
		title: string;
		author?: string | null;
		/** Where the back button leads. */
		backHref: string;
		/** Click on the back button; may take over the navigation. */
		onback?: (event: MouseEvent) => void;
		/** Hidden toolbars are slid away and taken out of the tab order. */
		visible: boolean;
		/** The book is open; before that only the way back works. */
		ready: boolean;
		/** Null when the browser has no fullscreen API (iPhone); the button is then left out. */
		fullscreen: boolean | null;
		onsearch: () => void;
		ontoc: () => void;
		onsettings: () => void;
		onfullscreen: () => void;
		onhelp: () => void;
	};
	let {
		title,
		author,
		backHref,
		onback,
		visible,
		ready,
		fullscreen,
		onsearch,
		ontoc,
		onsettings,
		onfullscreen,
		onhelp
	}: Props = $props();
</script>

<header class="toolbar" class:hidden={!visible} inert={!visible}>
	<a class="r-icon" href={backHref} onclick={onback} aria-label={t('reader.back')} title={t('reader.back')}>
		<ArrowLeft size={20} />
	</a>
	<div class="heading">
		<h1>{title}</h1>
		{#if author}<p>{author}</p>{/if}
	</div>
	<button
		class="r-icon"
		onclick={onsearch}
		disabled={!ready}
		aria-label={t('reader.search')}
		title={`${t('reader.search')} (/)`}
	>
		<Search size={19} />
	</button>
	<button
		class="r-icon"
		onclick={ontoc}
		disabled={!ready}
		aria-label={t('reader.toc')}
		title={`${t('reader.toc')} (T)`}
	>
		<List size={20} />
	</button>
	<button
		class="r-icon"
		onclick={onsettings}
		aria-label={t('reader.settings')}
		title={`${t('reader.settings')} (S)`}
	>
		<ALargeSmall size={22} />
	</button>
	{#if fullscreen !== null}
		{@const label = fullscreen ? t('reader.exitFullscreen') : t('reader.fullscreen')}
		<button class="r-icon" onclick={onfullscreen} aria-label={label} title={`${label} (F)`}>
			{#if fullscreen}<Minimize size={19} />{:else}<Maximize size={19} />{/if}
		</button>
	{/if}
	<!-- Only where there is likely a keyboard to use the shortcuts with. -->
	<button
		class="r-icon keys"
		onclick={onhelp}
		aria-label={t('reader.help')}
		title={`${t('reader.help')} (?)`}
	>
		<Keyboard size={20} />
	</button>
</header>

<style>
	.toolbar {
		position: absolute;
		inset: 0 0 auto 0;
		z-index: 4;
		display: flex;
		align-items: center;
		gap: 0.1rem;
		padding: calc(0.2rem + env(safe-area-inset-top)) calc(0.35rem + env(safe-area-inset-right)) 0.2rem
			calc(0.35rem + env(safe-area-inset-left));
		background: color-mix(in srgb, var(--r-surface) 94%, transparent);
		-webkit-backdrop-filter: blur(12px);
		backdrop-filter: blur(12px);
		border-bottom: 1px solid var(--r-border);
		transition:
			transform 0.2s ease,
			opacity 0.2s ease;
	}
	.toolbar.hidden {
		transform: translateY(-100%);
		opacity: 0;
	}
	.heading {
		flex: 1;
		min-width: 0;
		padding: 0 0.4rem;
	}
	h1,
	p {
		margin: 0;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	h1 {
		font-size: 0.98rem;
		line-height: 1.25;
	}
	p {
		font-size: 0.78rem;
		color: var(--r-muted);
	}
	.keys {
		display: none;
	}
	@media (hover: hover) and (pointer: fine) {
		.keys {
			display: inline-flex;
		}
	}
</style>
