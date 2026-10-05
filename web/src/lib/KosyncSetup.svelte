<script lang="ts">
	import { Check, Copy, KeyRound, Trash2 } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { t } from '#lib/i18n';

	let { username }: { username: string } = $props();

	let key = $state<string | null>(null);
	let loaded = $state(false);
	let busy = $state(false);
	let copied = $state<string | null>(null);
	let confirmRemove = $state(false);

	const server = $derived(typeof location !== 'undefined' ? `${location.origin}/api/kosync` : '/api/kosync');

	$effect(() => {
		fetch('/api/account/kosync-key')
			.then((r) => (r.ok ? r.json() : { key: null }))
			.then((v) => (key = v.key))
			.finally(() => (loaded = true));
	});

	async function create() {
		busy = true;
		try {
			const res = await fetch('/api/account/kosync-key', { method: 'POST' });
			if (res.ok) key = (await res.json()).key;
		} finally {
			busy = false;
		}
	}

	async function remove() {
		confirmRemove = false;
		const res = await fetch('/api/account/kosync-key', { method: 'DELETE' });
		if (res.ok) key = null;
	}

	async function copy(what: string, value: string) {
		try {
			await navigator.clipboard.writeText(value);
			copied = what;
			setTimeout(() => (copied = null), 1500);
		} catch {
			// The fields are selectable instead.
		}
	}
</script>

<section class="kosync">
	<h3>{t('kosync.heading')}</h3>
	<p class="intro">{t('kosync.intro')}</p>

	{#if loaded && !key}
		<button type="button" disabled={busy} onclick={create}>
			<KeyRound size={14} />
			{t('kosync.create')}
		</button>
	{:else if key}
		<dl>
			<dt>{t('kosync.server')}</dt>
			<dd>
				<input readonly value={server} onfocus={(e) => e.currentTarget.select()} />
				<button type="button" class="ghost" onclick={() => copy('server', server)} aria-label={t('kobo.copy')}>
					{#if copied === 'server'}<Check size={13} />{:else}<Copy size={13} />{/if}
				</button>
			</dd>
			<dt>{t('login.username')}</dt>
			<dd><input readonly value={username} /></dd>
			<dt>{t('kosync.key')}</dt>
			<dd>
				<input readonly class="mono" value={key} onfocus={(e) => e.currentTarget.select()} />
				<button type="button" class="ghost" onclick={() => copy('key', key ?? '')} aria-label={t('kobo.copy')}>
					{#if copied === 'key'}<Check size={13} />{:else}<Copy size={13} />{/if}
				</button>
			</dd>
		</dl>
		<ol class="steps">
			<li>{t('kosync.step1')}</li>
			<li>{t('kosync.step2')}</li>
			<li>{t('kosync.step3')}</li>
			<li>{t('kosync.step4')}</li>
		</ol>
		<p class="hint">{t('kosync.note')}</p>
		<div class="row">
			<button type="button" class="ghost" disabled={busy} onclick={create}>{t('kosync.renew')}</button>
			<button type="button" class="ghost danger" onclick={() => (confirmRemove = true)}>
				<Trash2 size={13} />
				{t('kosync.remove')}
			</button>
		</div>
	{/if}
</section>

<ConfirmDialog
	open={confirmRemove}
	title={t('kosync.remove')}
	message={t('kosync.removeConfirm')}
	confirmLabel={t('kosync.remove')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={remove}
	oncancel={() => (confirmRemove = false)}
/>

<style>
	.kosync {
		margin-top: 2rem;
		max-width: 32rem;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	h3 {
		font-size: 1rem;
		margin: 0;
	}
	.intro,
	.hint {
		margin: 0;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.hint {
		font-size: 0.8rem;
	}
	button {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		align-self: start;
	}
	dl {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 0.45rem 0.75rem;
		align-items: center;
		margin: 0;
	}
	dt {
		font-size: 0.85rem;
		color: var(--muted);
	}
	dd {
		margin: 0;
		display: flex;
		gap: 0.4rem;
		min-width: 0;
	}
	dd input {
		font-size: 0.85rem;
	}
	.mono {
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		letter-spacing: 0.04em;
	}
	dd button {
		flex-shrink: 0;
		padding: 0.3rem 0.5rem;
	}
	.steps {
		margin: 0;
		padding-left: 1.2rem;
		font-size: 0.88rem;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.row {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
	}
	.danger {
		color: var(--danger);
		border-color: var(--danger);
	}
</style>
