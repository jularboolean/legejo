<script lang="ts">
	// What changed in Legejo since a version, from the changelog built into
	// the server: shown after an upgrade, and from the admin page.
	import { X } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import { renderMarkdown } from '#lib/markdown';

	type Section = { version: string; date: string; body: string };
	let { since, onclose }: { since: string | null; onclose: () => void } = $props();

	let dialog: HTMLDialogElement;
	let sections = $state<Section[] | null>(null);
	let failed = $state(false);

	$effect(() => {
		if (!dialog.open) dialog.showModal();
		const url = since ? `/api/changelog?since=${encodeURIComponent(since)}` : '/api/changelog';
		fetch(url)
			.then(async (res) => {
				if (!res.ok) throw new Error();
				sections = (await res.json()).sections;
			})
			.catch(() => (failed = true));
	});
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dialog} aria-label={t('whatsnew.title')} {onclose} onclick={(e) => e.target === dialog && onclose()}>
	<div class="inner">
		<header>
			<h2>{t('whatsnew.title')}</h2>
			<button type="button" class="ghost icon" onclick={onclose} aria-label={t('common.close')} title={t('common.close')}><X size={18} /></button>
		</header>
		<div class="body">
			{#if failed}
				<p class="muted">{t('common.network')}</p>
			{:else if sections === null}
				<p class="muted" aria-busy="true"></p>
			{:else if sections.length === 0}
				<p class="muted">{t('whatsnew.none')}</p>
			{:else}
				{#each sections as s (s.version)}
					<h3>{s.version} <small>{s.date}</small></h3>
					<!-- eslint-disable-next-line svelte/no-at-html-tags -- the changelog is this server's own text -->
					{@html renderMarkdown(s.body)}
				{/each}
			{/if}
		</div>
	</div>
</dialog>

<style>
	dialog {
		width: min(38rem, calc(100vw - 2rem));
		max-height: min(42rem, calc(100dvh - 2rem));
		margin: auto;
		padding: 0;
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--card);
		color: var(--fg);
		box-shadow: var(--shadow);
	}
	dialog::backdrop {
		background: rgba(0, 0, 0, 0.35);
	}
	.inner {
		display: flex;
		flex-direction: column;
		max-height: min(42rem, calc(100dvh - 2rem));
	}
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.6rem 0.6rem 0.6rem 1.25rem;
		border-bottom: 1px solid var(--border);
	}
	h2 {
		margin: 0;
		font-size: 1.1rem;
	}
	.body {
		overflow-y: auto;
		padding: 0.25rem 1.25rem 1.25rem;
		font-size: 0.93rem;
	}
	h3 {
		font-size: 1rem;
		margin: 1.25rem 0 0.35rem;
	}
	h3 small {
		font-weight: 400;
		color: var(--muted);
		margin-left: 0.4rem;
	}
	.body :global(ul) {
		padding-left: 1.2rem;
	}
	.body :global(li) {
		margin: 0.3rem 0;
	}
	.body :global(code) {
		font-size: 0.86em;
	}
	.muted {
		color: var(--muted);
	}
</style>
