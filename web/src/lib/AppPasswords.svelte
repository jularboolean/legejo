<script lang="ts">
	import { Check, Copy, Plus, Trash2 } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { getLocale, t } from '#lib/i18n';

	type AppPassword = { id: number; name: string; created_at: string; last_used_at: string | null };

	let items = $state<AppPassword[]>([]);
	let name = $state('');
	let busy = $state(false);
	let fresh = $state<{ name: string; secret: string } | null>(null);
	let copied = $state(false);
	let removing = $state<AppPassword | null>(null);
	let errorMsg = $state('');

	const fmt = $derived(new Intl.DateTimeFormat(getLocale() === 'en' ? 'en-GB' : getLocale(), { dateStyle: 'medium' }));

	async function load() {
		const res = await fetch('/api/account/app-passwords');
		if (res.ok) items = await res.json();
	}
	$effect(() => {
		load();
	});

	async function create(e: SubmitEvent) {
		e.preventDefault();
		if (!name.trim()) return;
		busy = true;
		errorMsg = '';
		copied = false;
		try {
			const res = await fetch('/api/account/app-passwords', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ name: name.trim() })
			});
			if (!res.ok) {
				errorMsg = t('edit.saveFailed');
				return;
			}
			const v = await res.json();
			fresh = { name: v.name, secret: v.secret };
			name = '';
			await load();
		} catch {
			errorMsg = t('common.network');
		} finally {
			busy = false;
		}
	}

	async function remove() {
		if (!removing) return;
		const id = removing.id;
		removing = null;
		const res = await fetch(`/api/account/app-passwords/${id}`, { method: 'DELETE' });
		if (res.ok) {
			if (fresh && items.find((i) => i.id === id)?.name === fresh.name) fresh = null;
			await load();
		}
	}

	async function copy() {
		if (!fresh) return;
		try {
			await navigator.clipboard.writeText(fresh.secret);
			copied = true;
		} catch {
			// The field is selectable instead.
		}
	}
</script>

<div class="apppw">
	<h4>{t('apppw.heading')}</h4>
	<p class="hint">{t('apppw.intro')}</p>

	{#if fresh}
		<div class="fresh">
			<p>{t('apppw.created', { name: fresh.name })}</p>
			<div class="row">
				<input readonly class="mono" value={fresh.secret} onfocus={(e) => e.currentTarget.select()} />
				<button type="button" class="ghost" onclick={copy}>
					{#if copied}<Check size={13} /> {t('kobo.copied')}{:else}<Copy size={13} /> {t('kobo.copy')}{/if}
				</button>
			</div>
			<p class="hint">{t('apppw.once')}</p>
		</div>
	{/if}

	{#if items.length > 0}
		<ul>
			{#each items as item (item.id)}
				<li>
					<span class="name">{item.name}</span>
					<span class="meta">
						{item.last_used_at
							? t('apppw.lastUsed', { date: fmt.format(new Date(item.last_used_at)) })
							: t('apppw.neverUsed')}
					</span>
					<button type="button" class="ghost icon" aria-label={t('apppw.remove')} title={t('apppw.remove')} onclick={() => (removing = item)}>
						<Trash2 size={13} />
					</button>
				</li>
			{/each}
		</ul>
	{/if}

	<form onsubmit={create}>
		<input bind:value={name} maxlength="60" placeholder={t('apppw.namePlaceholder')} aria-label={t('apppw.name')} />
		<button type="submit" class="ghost" disabled={busy || !name.trim()}>
			<Plus size={13} />
			{t('apppw.create')}
		</button>
	</form>
	{#if errorMsg}<p class="error">{errorMsg}</p>{/if}
</div>

<ConfirmDialog
	open={removing !== null}
	title={t('apppw.remove')}
	message={t('apppw.removeConfirm', { name: removing?.name ?? '' })}
	confirmLabel={t('apppw.remove')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={remove}
	oncancel={() => (removing = null)}
/>

<style>
	.apppw {
		margin-top: 0.9rem;
		display: flex;
		flex-direction: column;
		gap: 0.55rem;
	}
	h4 {
		margin: 0;
		font-size: 0.92rem;
	}
	.hint {
		margin: 0;
		font-size: 0.8rem;
		color: var(--muted);
	}
	.fresh {
		background: var(--card);
		border: 1px solid var(--accent);
		border-radius: 8px;
		padding: 0.7rem 0.85rem;
		display: flex;
		flex-direction: column;
		gap: 0.45rem;
	}
	.fresh p {
		margin: 0;
		font-size: 0.88rem;
	}
	.row,
	form {
		display: flex;
		gap: 0.5rem;
	}
	.row button,
	form button {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
	}
	.mono {
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		letter-spacing: 0.05em;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
	}
	li {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.45rem 0.4rem 0.45rem 0.8rem;
		font-size: 0.88rem;
	}
	li + li {
		border-top: 1px solid var(--border);
	}
	.name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.meta {
		font-size: 0.75rem;
		color: var(--muted);
		white-space: nowrap;
	}
	.icon {
		padding: 0.25rem 0.4rem;
	}
</style>
