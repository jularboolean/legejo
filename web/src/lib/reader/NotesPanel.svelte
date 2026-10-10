<script lang="ts">
	// Every highlight and note in the book, in the order they were made.
	import { Download, Pencil } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import type { Annotation } from '#lib/types';
	import Sheet from './Sheet.svelte';

	type Props = {
		open: boolean;
		annotations: Annotation[];
		exportHref: string;
		/** Go to the passage. */
		onselect: (annotation: Annotation) => void;
		/** Open the passage for a note, or for removal. */
		onedit: (annotation: Annotation) => void;
		onclose: () => void;
	};
	let { open, annotations, exportHref, onselect, onedit, onclose }: Props = $props();
</script>

<Sheet {open} title={t('reader.notes')} side="right" {onclose}>
	{#snippet actions()}
		{#if annotations.length > 0}
			<a class="r-icon" href={exportHref} download title={t('reader.notes.export')} aria-label={t('reader.notes.export')}>
				<Download size={18} />
			</a>
		{/if}
	{/snippet}
	{#if annotations.length === 0}
		<p class="empty">{t('reader.notesEmpty')}</p>
	{:else}
		<ol>
			{#each annotations as a (a.id)}
				<li>
					<button class="passage" onclick={() => onselect(a)}>
						<q>{a.text}</q>
						{#if a.note}<span class="note">{a.note}</span>{/if}
					</button>
					<button class="r-icon edit" onclick={() => onedit(a)} aria-label={t('reader.notes.edit')} title={t('reader.notes.edit')}>
						<Pencil size={16} />
					</button>
				</li>
			{/each}
		</ol>
	{/if}
</Sheet>

<style>
	.empty {
		margin: 0;
		padding: 1rem 1.25rem;
		color: var(--r-muted);
		font-size: 0.92rem;
		line-height: 1.45;
	}
	ol {
		list-style: none;
		margin: 0;
		padding: 0.25rem 0 1rem;
	}
	li {
		display: flex;
		align-items: flex-start;
		gap: 0.25rem;
		padding: 0 0.6rem 0 0;
		border-bottom: 1px solid color-mix(in srgb, var(--r-border) 60%, transparent);
	}
	.passage {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		text-align: left;
		padding: 0.7rem 0.5rem 0.7rem 1.25rem;
		border-radius: 0;
		font-size: 0.92rem;
		line-height: 1.4;
	}
	.passage:hover {
		background: color-mix(in srgb, var(--r-fg) 5%, transparent);
	}
	q {
		quotes: '“' '”';
		display: -webkit-box;
		-webkit-line-clamp: 4;
		line-clamp: 4;
		-webkit-box-orient: vertical;
		overflow: hidden;
		border-left: 3px solid #f2c94c;
		padding-left: 0.6rem;
	}
	.note {
		color: var(--r-muted);
		white-space: pre-wrap;
	}
	.edit {
		margin-top: 0.45rem;
		color: var(--r-muted);
	}
</style>
