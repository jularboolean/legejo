<script lang="ts">
	// A closer look at a book that is not in the library yet: on a shelf
	// shared by link from another instance, or a federated one. The cover,
	// what is known about it, and the whole description, which may be HTML
	// from the EPUB or Markdown, shown as the library shows its own.
	import type { Snippet } from 'svelte';
	import { X } from '@lucide/svelte';
	import { fade, pop } from '#lib/motion';
	import { renderMarkdown } from '#lib/markdown';
	import { t } from '#lib/i18n';

	let {
		open,
		title,
		author = null,
		cover = null,
		facts = [],
		description = null,
		link = null,
		actions,
		onclose
	}: {
		open: boolean;
		title: string;
		author?: string | null;
		cover?: string | null;
		/** Label and value, in the order shown; empty values are left out. */
		facts?: { label: string; value: string | null | undefined }[];
		description?: string | null;
		/** The book's own page elsewhere. */
		link?: { href: string; label: string } | null;
		actions?: Snippet;
		onclose: () => void;
	} = $props();

	let closeBtn = $state<HTMLButtonElement | null>(null);
	let brokenCover = $state(false);
	const html = $derived(description ? renderMarkdown(description) : '');
	const shown = $derived(facts.filter((f) => f.value != null && String(f.value).trim() !== ''));

	$effect(() => {
		if (open) {
			brokenCover = false;
			closeBtn?.focus();
		}
	});

	function onkeydown(e: KeyboardEvent) {
		if (open && e.key === 'Escape') {
			e.preventDefault();
			onclose();
		}
	}
</script>

<svelte:window {onkeydown} />

{#if open}
	<div class="backdrop" onclick={onclose} role="presentation" transition:fade={{ duration: 160 }}>
		<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
		<div class="peek" role="dialog" aria-modal="true" aria-label={title} tabindex="-1" onclick={(e) => e.stopPropagation()} in:pop>
			<button type="button" class="close" bind:this={closeBtn} onclick={onclose} aria-label={t('common.close')} title={t('common.close')}>
				<X size={18} />
			</button>
			<div class="top">
				<div class="cover">
					{#if cover && !brokenCover}
						<img src={cover} alt="" onerror={() => (brokenCover = true)} />
					{:else}
						<span>{title}</span>
					{/if}
				</div>
				<div class="head">
					<h2>{title}</h2>
					{#if author}<p class="author">{author}</p>{/if}
					{#if shown.length > 0}
						<dl>
							{#each shown as f (f.label)}
								<dt>{f.label}</dt>
								<dd>{f.value}</dd>
							{/each}
						</dl>
					{/if}
					<div class="actions">
						{@render actions?.()}
						{#if link}<a class="elsewhere" href={link.href} target="_blank" rel="noreferrer">{link.label} ↗</a>{/if}
					</div>
				</div>
			</div>
			{#if html}
				<!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized by renderMarkdown -->
				<div class="description">{@html html}</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		background: rgba(20, 16, 10, 0.45);
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 1rem;
		z-index: 100;
	}
	.peek {
		position: relative;
		background: var(--card);
		border: 1px solid var(--border);
		border-radius: 12px;
		box-shadow: var(--shadow);
		padding: 1.5rem;
		width: 100%;
		max-width: 40rem;
		max-height: calc(100dvh - 2rem);
		overflow-y: auto;
	}
	.close {
		position: absolute;
		top: 0.6rem;
		right: 0.6rem;
		display: inline-flex;
		padding: 0.35rem;
		border: none;
		background: none;
		color: var(--muted);
	}
	.top {
		display: flex;
		gap: 1.25rem;
		align-items: flex-start;
	}
	.cover {
		flex: 0 0 9rem;
		aspect-ratio: 2 / 3;
		border-radius: 4px;
		overflow: hidden;
		background: var(--bg);
		border: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: center;
		text-align: center;
		font-size: 0.8rem;
		color: var(--muted);
		padding: 0.4rem;
	}
	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.head {
		flex: 1;
		min-width: 0;
	}
	h2 {
		margin: 0 1.5rem 0.25rem 0;
		font-size: 1.3rem;
		line-height: 1.25;
	}
	.author {
		margin: 0 0 0.75rem;
		color: var(--muted);
	}
	dl {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 0.2rem 0.75rem;
		margin: 0 0 1rem;
		font-size: 0.88rem;
	}
	dt {
		color: var(--muted);
	}
	dd {
		margin: 0;
		overflow-wrap: anywhere;
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.6rem;
	}
	.elsewhere {
		font-size: 0.88rem;
	}
	.description {
		margin-top: 1.25rem;
		padding-top: 1rem;
		border-top: 1px solid var(--border);
		line-height: 1.6;
		overflow-wrap: anywhere;
	}
	.description :global(p) {
		margin: 0 0 0.75rem;
	}
	.description :global(img) {
		max-width: 100%;
	}
	@media (max-width: 520px) {
		.top {
			flex-direction: column;
			align-items: center;
		}
		.cover {
			flex-basis: auto;
			width: 8rem;
		}
		.head {
			width: 100%;
		}
	}
</style>
