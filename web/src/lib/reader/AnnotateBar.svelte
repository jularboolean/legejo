<script lang="ts">
	// What to do with a passage: highlight it, give it a note, or, for a
	// highlight that exists, change or remove it. A bar at the foot of the
	// page, where the reading-aloud controls sit otherwise.
	import { tick } from 'svelte';
	import { Highlighter, MessageSquarePlus, Trash2, X } from '@lucide/svelte';
	import { t } from '#lib/i18n';

	type Props = {
		/** The passage: a selection waiting for an action, or a highlight being edited. */
		text: string;
		/** The note of the highlight being edited; null for a new passage. */
		note: string | null;
		editing: boolean;
		saving: boolean;
		error: string | null;
		/** Sits higher while the progress bar is expanded. */
		raised: boolean;
		/** A new highlight, with or without a note. */
		onhighlight: (note: string | null) => void;
		/** The note of an existing highlight. */
		onsave: (note: string | null) => void;
		ondelete: () => void;
		oncancel: () => void;
	};
	let { text, note, editing, saving, error, raised, onhighlight, onsave, ondelete, oncancel }: Props = $props();

	let writing = $state(false);
	let draft = $state('');
	let field = $state<HTMLTextAreaElement | null>(null);

	// A fresh passage starts without the note field; an existing note is shown at once.
	$effect(() => {
		writing = editing && note !== null;
		draft = note ?? '';
	});

	async function startNote() {
		writing = true;
		await tick();
		field?.focus();
	}

	function submit(e: SubmitEvent) {
		e.preventDefault();
		const value = draft.trim() || null;
		if (editing) onsave(value);
		else onhighlight(value);
	}

	const snippet = $derived(text.length > 160 ? `${text.slice(0, 160).trimEnd()}…` : text);
</script>

<form class="annotate" class:raised onsubmit={submit} aria-label={t('reader.notes')}>
	<div class="row">
		<p class="passage">“{snippet}”</p>
		<button type="button" class="r-icon" onclick={oncancel} aria-label={t('common.cancel')} title={t('common.cancel')}>
			<X size={18} />
		</button>
	</div>
	{#if writing}
		<textarea
			bind:this={field}
			bind:value={draft}
			rows="3"
			placeholder={t('reader.notes.notePlaceholder')}
			aria-label={t('reader.notes.note')}
			disabled={saving}
		></textarea>
	{/if}
	{#if error}<p class="error">{error}</p>{/if}
	<div class="row actions">
		{#if editing}
			<button type="button" class="act danger" onclick={ondelete} disabled={saving}>
				<Trash2 size={15} />
				{t('reader.notes.delete')}
			</button>
			{#if !writing}
				<button type="button" class="act" onclick={startNote} disabled={saving}>
					<MessageSquarePlus size={15} />
					{t('reader.notes.addNote')}
				</button>
			{:else}
				<button type="submit" class="act main" disabled={saving}>{t('reader.notes.save')}</button>
			{/if}
		{:else if writing}
			<button type="submit" class="act main" disabled={saving}>
				<Highlighter size={15} />
				{t('reader.notes.saveWithNote')}
			</button>
		{:else}
			<button type="submit" class="act main" disabled={saving}>
				<Highlighter size={15} />
				{t('reader.notes.highlight')}
			</button>
			<button type="button" class="act" onclick={startNote} disabled={saving}>
				<MessageSquarePlus size={15} />
				{t('reader.notes.addNote')}
			</button>
		{/if}
	</div>
</form>

<style>
	.annotate {
		position: absolute;
		z-index: 3;
		left: 50%;
		bottom: calc(1.4rem + env(safe-area-inset-bottom));
		transform: translateX(-50%);
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		width: min(34rem, calc(100% - 1rem));
		padding: 0.6rem 0.6rem 0.6rem 0.9rem;
		border-radius: 14px;
		background: var(--r-surface);
		color: var(--r-fg);
		border: 1px solid var(--r-border);
		box-shadow: 0 2px 14px rgba(0, 0, 0, 0.16);
		transition: bottom 0.2s ease;
		animation: rise 0.18s ease;
	}
	/* Clear of the expanded progress bar. */
	.annotate.raised {
		bottom: calc(5.4rem + env(safe-area-inset-bottom));
	}
	.row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.passage {
		flex: 1;
		min-width: 0;
		margin: 0;
		font-size: 0.86rem;
		line-height: 1.35;
		color: var(--r-muted);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	textarea {
		width: 100%;
		resize: vertical;
		font: inherit;
		font-size: 0.92rem;
		padding: 0.45rem 0.6rem;
		border-radius: 8px;
		border: 1px solid var(--r-border);
		background: color-mix(in srgb, var(--r-surface) 80%, var(--r-fg) 4%);
		color: var(--r-fg);
	}
	.actions {
		justify-content: flex-end;
		flex-wrap: wrap;
	}
	.act {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		padding: 0.4rem 0.75rem;
		border: 1px solid var(--r-border);
		border-radius: 99px;
		font-size: 0.86rem;
	}
	.act.main {
		background: var(--r-link);
		color: var(--r-surface);
		border-color: var(--r-link);
	}
	.act.danger {
		margin-right: auto;
		color: var(--r-muted);
	}
	.error {
		margin: 0;
		font-size: 0.82rem;
		color: #c0392b;
	}
	@keyframes rise {
		from {
			opacity: 0;
			translate: 0 0.5rem;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.annotate {
			animation: none;
			transition: none;
		}
	}
</style>
