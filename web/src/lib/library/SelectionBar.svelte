<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { BookMarked, Bookmark, Download, Tag, Trash2, X } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { t } from '#lib/i18n';
	import { reasonText, type NotFederable } from '#lib/license';
	import type { Shelf } from '#lib/types';
	import Popover from './Popover.svelte';

	let {
		selected = $bindable(),
		visibleIds,
		shelves,
		ondone
	}: {
		selected: Set<number>;
		/** The books currently shown, for "select all". */
		visibleIds: number[];
		shelves: Shelf[];
		/** Leave selection mode. */
		ondone: () => void;
	} = $props();

	let busy = $state(false);
	let message = $state('');
	let skipped = $state<{ id: number; title: string; reason: NotFederable }[]>([]);
	let tag = $state('');
	let confirmDelete = $state(false);

	const ids = $derived([...selected]);
	const allShown = $derived(visibleIds.length > 0 && visibleIds.every((id) => selected.has(id)));

	function toggleAll() {
		selected = allShown ? new Set() : new Set(visibleIds);
	}

	async function run(action: Record<string, unknown>, done: (n: number) => string) {
		if (ids.length === 0) return;
		busy = true;
		message = '';
		skipped = [];
		try {
			const res = await fetch('/api/books/bulk', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ ids, ...action })
			});
			const body = await res.json().catch(() => null);
			if (!res.ok) {
				message = t('edit.saveFailed');
				return;
			}
			message = done(body.done);
			skipped = body.skipped ?? [];
			await invalidateAll();
		} catch {
			message = t('common.network');
		} finally {
			busy = false;
		}
	}

	const archiveHref = $derived(`/api/books/archive?ids=${ids.join(',')}`);
</script>

<div class="bar" role="toolbar" aria-label={t('select.toolbar')}>
	<div class="inner">
		<span class="count">{t('select.count', { count: selected.size })}</span>
		<button type="button" class="link" onclick={toggleAll}>
			{allShown ? t('select.none') : t('select.all', { count: visibleIds.length })}
		</button>

		<div class="actions">
			<Popover label={t('select.shelf')} align="right" up>
				{#snippet button()}<BookMarked size={13} /> {t('select.shelf')}{/snippet}
				{#snippet children(close)}
					<div class="menu">
						{#each shelves as shelf (shelf.id)}
							<div class="shelf-row">
								<span class="shelf-name">{shelf.name}</span>
								<button
									type="button"
									class="mini"
									disabled={busy || selected.size === 0}
									onclick={() => {
										close();
										run({ action: 'add_to_shelf', shelf_id: shelf.id }, (n) =>
											t('select.shelved', { count: n, name: shelf.name })
										);
									}}>{t('select.add')}</button
								>
								<button
									type="button"
									class="mini ghost"
									disabled={busy || selected.size === 0}
									onclick={() => {
										close();
										run({ action: 'remove_from_shelf', shelf_id: shelf.id }, (n) =>
											t('select.unshelved', { count: n, name: shelf.name })
										);
									}}>{t('select.remove')}</button
								>
							</div>
						{:else}
							<p class="empty">{t('sidebar.noShelves')}</p>
						{/each}
					</div>
				{/snippet}
			</Popover>

			<Popover label={t('select.tag')} align="right" up>
				{#snippet button()}<Tag size={13} /> {t('select.tag')}{/snippet}
				{#snippet children(close)}
					<form
						class="tag-form"
						onsubmit={(e) => {
							e.preventDefault();
							const value = tag.trim();
							if (!value) return;
							close();
							run({ action: 'add_tag', tag: value }, (n) => t('select.tagged', { count: n, tag: value }));
						}}
					>
						<input bind:value={tag} placeholder={t('select.tagPlaceholder')} aria-label={t('select.tag')} />
						<div class="tag-buttons">
							<button type="submit" class="mini" disabled={busy || !tag.trim() || selected.size === 0}>
								{t('select.add')}
							</button>
							<button
								type="button"
								class="mini ghost"
								disabled={busy || !tag.trim() || selected.size === 0}
								onclick={() => {
									const value = tag.trim();
									close();
									run({ action: 'remove_tag', tag: value }, (n) => t('select.untagged', { count: n, tag: value }));
								}}>{t('select.remove')}</button
							>
						</div>
					</form>
				{/snippet}
			</Popover>

			<Popover label={t('select.want')} align="right" up>
				{#snippet button()}<Bookmark size={13} /> {t('select.want')}{/snippet}
				{#snippet children(close)}
					<div class="menu">
						<button
							type="button"
							class="item"
							disabled={busy || selected.size === 0}
							onclick={() => {
								close();
								run({ action: 'want', want: true }, (n) => t('select.wanted', { count: n }));
							}}>{t('select.wantAdd')}</button
						>
						<button
							type="button"
							class="item"
							disabled={busy || selected.size === 0}
							onclick={() => {
								close();
								run({ action: 'want', want: false }, (n) => t('select.unwanted', { count: n }));
							}}>{t('select.wantRemove')}</button
						>
					</div>
				{/snippet}
			</Popover>

			<a
				class="button ghost"
				class:disabled={selected.size === 0}
				href={selected.size > 0 ? archiveHref : undefined}
				download="legejo.zip"
			>
				<Download size={13} />
				{t('select.download')}
			</a>
			<button type="button" class="ghost danger" disabled={busy || selected.size === 0} onclick={() => (confirmDelete = true)}>
				<Trash2 size={13} />
				{t('select.delete')}
			</button>
			<button type="button" class="ghost icon" onclick={ondone} aria-label={t('select.done')} title={t('select.done')}>
				<X size={15} />
			</button>
		</div>
	</div>
	{#if message || skipped.length > 0}
		<div class="feedback">
			{#if message}<span>{message}</span>{/if}
			{#if skipped.length > 0}
				<span class="skipped">
					{t('select.skipped', { count: skipped.length })}
					{#each skipped.slice(0, 3) as s (s.id)}
						<a href={`/books/${s.id}/edit`}>{s.title}</a> ({reasonText(s.reason)})
					{/each}
				</span>
			{/if}
		</div>
	{/if}
</div>

<ConfirmDialog
	open={confirmDelete}
	title={t('select.delete')}
	message={t('select.deleteConfirm', { count: selected.size })}
	confirmLabel={t('select.delete')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={() => {
		confirmDelete = false;
		run({ action: 'delete' }, (n) => t('select.deleted', { count: n })).then(() => (selected = new Set()));
	}}
	oncancel={() => (confirmDelete = false)}
/>

<style>
	.bar {
		position: sticky;
		bottom: 0;
		z-index: 30;
		margin: 1.5rem -1rem 0;
		padding: 0.6rem 1rem calc(0.6rem + env(safe-area-inset-bottom));
		background: var(--card);
		border-top: 1px solid var(--border);
		box-shadow: 0 -0.4rem 1.2rem rgb(0 0 0 / 0.08);
	}
	.inner {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.5rem 0.9rem;
	}
	.count {
		font-weight: 600;
		font-size: 0.9rem;
	}
	.link {
		background: none;
		border: none;
		padding: 0;
		color: var(--accent);
		font-size: 0.85rem;
		font-weight: 500;
	}
	.link:hover {
		text-decoration: underline;
		filter: none;
	}
	.actions {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.4rem;
		margin-left: auto;
	}
	.actions :global(button),
	.button {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.82rem;
	}
	.button {
		padding: 0.35rem 0.7rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--fg);
	}
	.button:hover {
		text-decoration: none;
		border-color: var(--muted);
	}
	.button.disabled {
		opacity: 0.5;
		pointer-events: none;
	}
	.danger {
		color: var(--danger);
		border-color: var(--danger);
	}
	.icon {
		padding: 0.3rem;
	}
	.menu {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		min-width: 15rem;
		max-height: 18rem;
		overflow-y: auto;
		padding: 0.3rem;
	}
	.shelf-row {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		padding: 0.15rem 0.2rem;
	}
	.shelf-name {
		flex: 1;
		min-width: 0;
		font-size: 0.85rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.mini {
		padding: 0.15rem 0.5rem;
		font-size: 0.75rem;
	}
	.item {
		background: none;
		border: none;
		color: var(--fg);
		text-align: left;
		padding: 0.4rem 0.5rem;
		font-size: 0.85rem;
		font-weight: 400;
		border-radius: 5px;
	}
	.item:hover {
		background: var(--bg);
		filter: none;
	}
	.tag-form {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		padding: 0.4rem;
		min-width: 14rem;
	}
	.tag-buttons {
		display: flex;
		gap: 0.35rem;
	}
	.empty {
		margin: 0.3rem;
		font-size: 0.82rem;
		color: var(--muted);
	}
	.feedback {
		margin-top: 0.4rem;
		font-size: 0.82rem;
		color: var(--muted);
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
	}
	.skipped {
		color: var(--danger);
	}
</style>
