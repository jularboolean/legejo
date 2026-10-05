<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { Download, FileJson, FileSpreadsheet, PackageOpen, Trash2 } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { getLocale, t } from '#lib/i18n';

	type Export = {
		id: number;
		status: 'queued' | 'running' | 'done' | 'failed' | 'expired';
		total: number;
		done: number;
		bytes: number;
		parts: { name: string; size: number }[];
		error: string | null;
		created_at: string;
		finished_at: string | null;
		expires_at: string | null;
	};

	let current = $state<Export | null>(null);
	let library = $state<{ count: number; bytes: number } | null>(null);
	let busy = $state(false);
	let errorMsg = $state('');

	function size(bytes: number): string {
		if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
		if (bytes >= 1024 ** 2) return `${Math.round(bytes / 1024 ** 2)} MB`;
		return `${Math.max(1, Math.round(bytes / 1024))} kB`;
	}
	const fmtDate = $derived(
		new Intl.DateTimeFormat(getLocale() === 'en' ? 'en-GB' : getLocale(), { dateStyle: 'long' })
	);

	async function refresh() {
		try {
			const res = await fetch('/api/account/export');
			if (res.ok) current = await res.json();
		} catch {
			// The next poll tries again.
		}
	}

	// Initial state, library size for the estimate, and polling while it runs.
	$effect(() => {
		refresh();
		fetch('/api/books')
			.then((r) => (r.ok ? r.json() : []))
			.then((books: { file_size: number }[]) => {
				library = { count: books.length, bytes: books.reduce((s, b) => s + (b.file_size ?? 0), 0) };
			})
			.catch(() => {});
	});
	$effect(() => {
		if (current?.status !== 'queued' && current?.status !== 'running') return;
		const timer = setInterval(refresh, 2000);
		return () => clearInterval(timer);
	});

	async function startExport() {
		busy = true;
		errorMsg = '';
		try {
			const res = await fetch('/api/account/export', { method: 'POST' });
			if (res.ok) current = await res.json();
			else errorMsg = t('edit.saveFailed');
		} catch {
			errorMsg = t('common.network');
		} finally {
			busy = false;
		}
	}

	async function discard() {
		const res = await fetch('/api/account/export', { method: 'DELETE' });
		if (res.ok) current = null;
	}

	const percent = $derived(current && current.total > 0 ? Math.round((current.done / current.total) * 100) : 0);

	// ---- Delete account
	let password = $state('');
	let understood = $state(false);
	let confirmOpen = $state(false);
	let deleteError = $state('');

	async function deleteAccount() {
		confirmOpen = false;
		deleteError = '';
		try {
			const res = await fetch('/api/account', {
				method: 'DELETE',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ password })
			});
			if (res.ok) {
				await invalidateAll();
				goto('/login');
				return;
			}
			const body = await res.json().catch(() => null);
			deleteError =
				body?.error === 'wrong current password'
					? t('leave.wrongPassword')
					: body?.error === 'the last admin cannot delete their account'
						? t('leave.lastAdmin')
						: t('edit.saveFailed');
		} catch {
			deleteError = t('common.network');
		}
	}
</script>

<section class="leave">
	<h2>{t('leave.exportHeading')}</h2>
	<p class="intro">{t('leave.exportIntro')}</p>

	<div class="row">
		<a class="button ghost" href="/api/account/metadata?format=csv" download>
			<FileSpreadsheet size={14} />
			{t('leave.csv')}
		</a>
		<a class="button ghost" href="/api/account/metadata?format=json" download>
			<FileJson size={14} />
			{t('leave.json')}
		</a>
	</div>

	<div class="export">
		{#if !current || current.status === 'expired'}
			<p class="hint">
				{#if library}{t('leave.estimate', { count: library.count, size: size(library.bytes) })}{/if}
				{t('leave.how')}
			</p>
			<button type="button" disabled={busy} onclick={startExport}>
				<PackageOpen size={14} />
				{t('leave.start')}
			</button>
		{:else if current.status === 'queued' || current.status === 'running'}
			<p>{current.status === 'queued' ? t('leave.queued') : t('leave.running', { done: current.done, total: current.total })}</p>
			<div class="progress" role="progressbar" aria-valuenow={percent} aria-valuemin="0" aria-valuemax="100">
				<span style:width={`${percent}%`}></span>
			</div>
			<p class="hint">{t('leave.canLeave')}</p>
		{:else if current.status === 'done'}
			<p>
				{t('leave.ready', { count: current.total, size: size(current.bytes) })}
				{#if current.expires_at}{t('leave.expires', { date: fmtDate.format(new Date(current.expires_at)) })}{/if}
			</p>
			<ul class="parts">
				{#each current.parts as part (part.name)}
					<li>
						<a href={`/api/account/export/${current.id}/${encodeURIComponent(part.name)}`} download>
							<Download size={14} />
							{part.name}
						</a>
						<span class="size">{size(part.size)}</span>
					</li>
				{/each}
			</ul>
			{#if current.parts.length > 1}<p class="hint">{t('leave.manyParts')}</p>{/if}
			<p class="hint">{t('leave.resume')}</p>
			<div class="row">
				<button type="button" class="ghost" disabled={busy} onclick={startExport}>{t('leave.again')}</button>
				<button type="button" class="ghost" onclick={discard}>{t('leave.discard')}</button>
			</div>
		{:else if current.status === 'failed'}
			<p class="error">{t('leave.failed')}</p>
			<button type="button" disabled={busy} onclick={startExport}>{t('leave.again')}</button>
		{/if}
		{#if errorMsg}<p class="error">{errorMsg}</p>{/if}
	</div>
</section>

<section class="leave danger-zone">
	<h2>{t('leave.deleteHeading')}</h2>
	<p class="intro">{t('leave.deleteIntro')}</p>
	<label>
		{t('account.currentPassword')}
		<input type="password" bind:value={password} autocomplete="current-password" />
	</label>
	<label class="check">
		<input type="checkbox" bind:checked={understood} />
		{t('leave.understood')}
	</label>
	{#if deleteError}<p class="error">{deleteError}</p>{/if}
	<button type="button" class="ghost danger" disabled={!password || !understood} onclick={() => (confirmOpen = true)}>
		<Trash2 size={13} />
		{t('leave.delete')}
	</button>
</section>

<ConfirmDialog
	open={confirmOpen}
	title={t('leave.deleteHeading')}
	message={t('leave.confirm')}
	confirmLabel={t('leave.delete')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={deleteAccount}
	oncancel={() => (confirmOpen = false)}
/>

<style>
	.leave {
		margin-top: 2.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--border);
		max-width: 32rem;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	h2 {
		font-size: 1.1rem;
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
	.row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.button,
	button {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		align-self: start;
	}
	.button {
		padding: 0.4rem 0.8rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--fg);
		font-size: 0.88rem;
	}
	.button:hover {
		text-decoration: none;
		border-color: var(--muted);
	}
	.export {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		background: var(--card);
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 0.9rem 1rem;
	}
	.export p {
		margin: 0;
		font-size: 0.9rem;
	}
	.progress {
		height: 0.5rem;
		background: var(--bg);
		border-radius: 99px;
		overflow: hidden;
	}
	.progress span {
		display: block;
		height: 100%;
		background: var(--accent);
		transition: width 0.4s ease;
	}
	.parts {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.parts li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
	}
	.parts a {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
	}
	.size {
		font-size: 0.8rem;
		color: var(--muted);
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.9rem;
		color: var(--muted);
		max-width: 20rem;
	}
	.check {
		flex-direction: row;
		align-items: center;
		gap: 0.45rem;
		color: var(--fg);
		max-width: none;
	}
	.danger {
		color: var(--danger);
		border-color: var(--danger);
	}
</style>
