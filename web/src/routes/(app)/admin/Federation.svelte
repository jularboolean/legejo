<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { Ban, Check, Search, Trash2 } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { t } from '#lib/i18n';
	import type {
		FedAdminSettings,
		FedInstance,
		FedInstancePreview,
		FedMode,
		FedOverview,
		FedRequest
	} from '#lib/types';

	let {
		settings,
		instances,
		overview
	}: {
		settings: FedAdminSettings;
		instances: FedInstance[];
		overview: FedOverview | null;
	} = $props();

	const MODES: { value: FedMode; key: Parameters<typeof t>[0] }[] = [
		{ value: 'off', key: 'fedAdmin.mode.off' },
		{ value: 'allowlist', key: 'fedAdmin.mode.allowlist' },
		{ value: 'open', key: 'fedAdmin.mode.open' }
	];

	// ---- Settings ----
	let saved = $state<FedAdminSettings>({ ...settings });
	let form = $state({
		mode: settings.mode,
		contact: settings.contact ?? '',
		max_epub_mb: String(settings.max_epub_mb)
	});
	let saving = $state(false);
	let settingsMsg = $state('');
	let settingsError = $state('');
	let confirmOn = $state(false);

	function submitSettings(e: SubmitEvent) {
		e.preventDefault();
		settingsMsg = '';
		settingsError = '';
		const mb = Number(form.max_epub_mb);
		if (!Number.isFinite(mb) || mb <= 0) {
			settingsError = t('fedAdmin.maxEpubInvalid');
			return;
		}
		// Leaving "off" makes the instance publicly discoverable: ask first.
		if (saved.mode === 'off' && form.mode !== 'off') {
			confirmOn = true;
			return;
		}
		saveSettings();
	}

	async function saveSettings() {
		confirmOn = false;
		saving = true;
		try {
			const res = await fetch('/api/admin/federation', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					mode: form.mode,
					contact: form.contact.trim() || null,
					max_epub_mb: Number(form.max_epub_mb)
				})
			});
			const body = await res.json().catch(() => null);
			if (!res.ok) {
				settingsError =
					body?.error === 'no public url'
						? t('fedAdmin.noPublicUrlError')
						: (body?.error ?? t('edit.saveFailed'));
				return;
			}
			saved = body;
			form = {
				mode: saved.mode,
				contact: saved.contact ?? '',
				max_epub_mb: String(saved.max_epub_mb)
			};
			settingsMsg = t('account.saved');
			// The sidebar link and the shelf editor follow /api/fed/status.
			await invalidateAll();
		} catch {
			settingsError = t('common.network');
		} finally {
			saving = false;
		}
	}

	// ---- Connect an instance ----
	let domain = $state('');
	let previewing = $state(false);
	let preview = $state<FedInstancePreview | null>(null);
	let previewError = $state('');

	async function lookup(e: SubmitEvent) {
		e.preventDefault();
		const d = domain.trim();
		if (!d) return;
		previewing = true;
		preview = null;
		previewError = '';
		try {
			const res = await fetch('/api/admin/federation/preview', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ domain: d })
			});
			const body = await res.json().catch(() => null);
			if (res.ok && body) {
				preview = body;
			} else {
				previewError = body?.error ?? t('fedAdmin.previewFailed');
			}
		} catch {
			previewError = t('common.network');
		} finally {
			previewing = false;
		}
	}

	let listError = $state('');

	async function setStatus(d: string, status: 'allowed' | 'blocked') {
		listError = '';
		try {
			const res = await fetch(`/api/admin/federation/instances/${encodeURIComponent(d)}`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ status })
			});
			if (!res.ok) {
				const body = await res.json().catch(() => null);
				listError = body?.error ?? t('edit.saveFailed');
				return false;
			}
			await invalidateAll();
			return true;
		} catch {
			listError = t('common.network');
			return false;
		}
	}

	async function allowPreviewed() {
		if (!preview) return;
		if (await setStatus(preview.domain, 'allowed')) {
			preview = null;
			domain = '';
		}
	}

	// ---- Instances awaiting a decision ----
	const requestsByDomain = $derived.by(() => {
		const map = new Map<string, FedRequest[]>();
		for (const r of overview?.requests ?? []) {
			map.set(r.domain, [...(map.get(r.domain) ?? []), r]);
		}
		return [...map.entries()];
	});

	async function dismiss(d: string) {
		listError = '';
		try {
			const res = await fetch(`/api/admin/federation/requests/${encodeURIComponent(d)}`, { method: 'DELETE' });
			if (!res.ok) {
				listError = t('edit.saveFailed');
				return;
			}
			await invalidateAll();
		} catch {
			listError = t('common.network');
		}
	}

	// ---- Block / remove, both behind a confirm ----
	let pending = $state<{ kind: 'block' | 'remove'; domain: string } | null>(null);

	async function confirmPending() {
		const p = pending;
		pending = null;
		if (!p) return;
		if (p.kind === 'block') {
			await setStatus(p.domain, 'blocked');
			return;
		}
		listError = '';
		try {
			const res = await fetch(`/api/admin/federation/instances/${encodeURIComponent(p.domain)}`, {
				method: 'DELETE'
			});
			if (!res.ok) {
				listError = t('edit.saveFailed');
				return;
			}
			await invalidateAll();
		} catch {
			listError = t('common.network');
		}
	}

	function day(ts: string | null): string {
		return ts ? ts.slice(0, 10) : '—';
	}
	function minute(ts: string): string {
		return ts.replace('T', ' ').slice(0, 16);
	}
</script>

<section class="fed">
	<h2>{t('fedAdmin.heading')}</h2>

	{#if saved.public_url}
		<p class="hint">{t('fedAdmin.publicUrl')} <code>{saved.public_url}</code></p>
	{:else}
		<p class="warn">{t('fedAdmin.noPublicUrl')}</p>
	{/if}

	<form class="settings" onsubmit={submitSettings}>
		<label>
			{t('fedAdmin.mode')}
			<select bind:value={form.mode}>
				{#each MODES as m (m.value)}
					<option value={m.value}>{t(m.key)}</option>
				{/each}
			</select>
			<span class="hint">
				{form.mode === 'off'
					? t('fedAdmin.mode.offHint')
					: form.mode === 'allowlist'
						? t('fedAdmin.mode.allowlistHint')
						: t('fedAdmin.mode.openHint')}
			</span>
		</label>
		<label>
			{t('fedAdmin.contact')}
			<input bind:value={form.contact} placeholder="admin@example.org" />
			<span class="hint">{t('fedAdmin.contactHint')}</span>
		</label>
		<label class="narrow">
			{t('fedAdmin.maxEpub')}
			<input bind:value={form.max_epub_mb} inputmode="numeric" />
		</label>
		<div class="row">
			<button type="submit" disabled={saving}>{saving ? t('edit.saving') : t('edit.save')}</button>
			{#if settingsMsg}<span class="ok">{settingsMsg}</span>{/if}
		</div>
		{#if settingsError}<p class="error">{settingsError}</p>{/if}
	</form>

	{#if requestsByDomain.length > 0}
		<h3>{t('fedAdmin.requests')}</h3>
		<p class="hint">{t('fedAdmin.requestsHint')}</p>
		<ul class="requests">
			{#each requestsByDomain as [d, items] (d)}
				<li>
					<div class="request-info">
						<strong>{d}</strong>
						{#each items as r (r.direction + r.detail + r.requested_by)}
							<span class="request-line">
								{r.direction === 'out'
									? t('fedAdmin.requestOut', { user: r.requested_by, shelf: r.detail })
									: t('fedAdmin.requestIn', { shelf: r.detail })}
								<span class="muted">· {minute(r.last_at)}</span>
							</span>
						{/each}
					</div>
					<div class="request-actions">
						<button type="button" onclick={() => setStatus(d, 'allowed')}>
							<Check size={13} />
							{t('fedAdmin.allow')}
						</button>
						<button type="button" class="ghost" onclick={() => (pending = { kind: 'block', domain: d })}>
							<Ban size={13} />
							{t('fedAdmin.block')}
						</button>
						<button type="button" class="ghost" onclick={() => dismiss(d)}>{t('fedAdmin.dismiss')}</button>
					</div>
				</li>
			{/each}
		</ul>
	{/if}

	<h3>{t('fedAdmin.connect')}</h3>
	<form class="row" onsubmit={lookup}>
		<input
			bind:value={domain}
			placeholder="legejo.example.org"
			autocapitalize="off"
			autocomplete="off"
			spellcheck="false"
		/>
		<button type="submit" class="ghost" disabled={previewing || !domain.trim()}>
			<Search size={13} />
			{previewing ? t('fedAdmin.looking') : t('fedAdmin.lookup')}
		</button>
	</form>
	<p class="hint">{t('fedAdmin.connectHint')}</p>
	{#if previewError}<p class="error">{previewError}</p>{/if}
	{#if preview}
		<div class="preview">
			<dl>
				<dt>{t('fedAdmin.domain')}</dt>
				<dd>{preview.domain}</dd>
				<dt>{t('fedAdmin.software')}</dt>
				<dd>{preview.software ?? '—'}{preview.version ? ` ${preview.version}` : ''}</dd>
				<dt>{t('fedAdmin.contact')}</dt>
				<dd>{preview.contact ?? '—'}</dd>
				{#if preview.federation_mode}
					<dt>{t('fedAdmin.theirMode')}</dt>
					<dd>{preview.federation_mode}</dd>
				{/if}
			</dl>
			<div class="row">
				<button type="button" onclick={allowPreviewed}>
					<Check size={13} />
					{t('fedAdmin.allow')}
				</button>
				<button type="button" class="ghost" onclick={() => (preview = null)}>
					{t('common.cancel')}
				</button>
			</div>
		</div>
	{/if}

	<h3>{t('fedAdmin.instances')}</h3>
	{#if listError}<p class="error">{listError}</p>{/if}
	{#if instances.length === 0}
		<p class="hint">{t('fedAdmin.noInstances')}</p>
	{:else}
		<div class="scroll">
			<table>
				<thead>
					<tr>
						<th>{t('fedAdmin.domain')}</th>
						<th>{t('fedAdmin.status')}</th>
						<th>{t('fedAdmin.software')}</th>
						<th>{t('fedAdmin.lastSeen')}</th>
						<th></th>
					</tr>
				</thead>
				<tbody>
					{#each instances as inst (inst.domain)}
						<tr>
							<td>
								<span class="domain">{inst.domain}</span>
								{#if inst.note}<span class="note">{inst.note}</span>{/if}
							</td>
							<td>
								<span class="badge" class:bad={inst.status === 'blocked'}>
									{inst.status === 'allowed' ? t('fedAdmin.allowed') : t('fedAdmin.blocked')}
								</span>
							</td>
							<td class="muted">{inst.software ?? '—'}</td>
							<td class="muted">{day(inst.last_seen_at)}</td>
							<td class="actions">
								{#if inst.status === 'allowed'}
									<button
										type="button"
										class="ghost danger"
										onclick={() => (pending = { kind: 'block', domain: inst.domain })}
									>
										<Ban size={12} />
										{t('fedAdmin.block')}
									</button>
								{:else}
									<button type="button" class="ghost" onclick={() => setStatus(inst.domain, 'allowed')}>
										<Check size={12} />
										{t('fedAdmin.allow')}
									</button>
								{/if}
								<button
									type="button"
									class="ghost icon"
									title={t('fedAdmin.remove')}
									aria-label={t('fedAdmin.remove')}
									onclick={() => (pending = { kind: 'remove', domain: inst.domain })}
								>
									<Trash2 size={12} />
								</button>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}

	{#if overview}
		<h3>{t('fedAdmin.overview')}</h3>
		<p class="hint">
			{t('fedAdmin.queue', { count: overview.queue.length })}
			{#if overview.queue.oldest}· {t('fedAdmin.queueOldest', { at: minute(overview.queue.oldest) })}{/if}
		</p>

		<h4>{t('fedAdmin.fedShelves')}</h4>
		{#if overview.shelves.length === 0}
			<p class="hint">{t('fedAdmin.noFedShelves')}</p>
		{:else}
			<div class="scroll">
				<table>
					<thead>
						<tr>
							<th>{t('fedAdmin.shelf')}</th>
							<th>{t('admin.user')}</th>
							<th class="num">{t('fedAdmin.followers')}</th>
						</tr>
					</thead>
					<tbody>
						{#each overview.shelves as s (s.id)}
							<tr>
								<td>
									{s.name}
									{#if s.handle}<span class="note mono">{s.handle}</span>{/if}
								</td>
								<td class="muted">{s.owner}</td>
								<td class="num">{s.followers}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}

		<h4>{t('fedAdmin.rejections')}</h4>
		{#if overview.rejections.length === 0}
			<p class="hint">{t('fedAdmin.noRejections')}</p>
		{:else}
			<div class="scroll">
				<table class="small">
					<thead>
						<tr>
							<th>{t('fedAdmin.when')}</th>
							<th>{t('fedAdmin.type')}</th>
							<th>{t('fedAdmin.actor')}</th>
							<th>{t('fedAdmin.reason')}</th>
						</tr>
					</thead>
					<tbody>
						{#each overview.rejections as r, i (i)}
							<tr>
								<td class="muted nowrap">{minute(r.at)}</td>
								<td>{r.activity_type}</td>
								<td>
									<span class="domain">{r.domain}</span>
									<span class="note mono">{r.actor}</span>
								</td>
								<td>{r.reason}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	{/if}
</section>

<ConfirmDialog
	open={confirmOn}
	title={t('fedAdmin.confirmOnTitle')}
	message={t('fedAdmin.confirmOn')}
	confirmLabel={t('fedAdmin.confirmOnButton')}
	cancelLabel={t('common.cancel')}
	onconfirm={saveSettings}
	oncancel={() => (confirmOn = false)}
/>

<ConfirmDialog
	open={pending != null}
	title={pending?.kind === 'block' ? t('fedAdmin.block') : t('fedAdmin.remove')}
	message={pending
		? t(pending.kind === 'block' ? 'fedAdmin.blockConfirm' : 'fedAdmin.removeConfirm', {
				domain: pending.domain
			})
		: ''}
	confirmLabel={pending?.kind === 'block' ? t('fedAdmin.block') : t('fedAdmin.remove')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={confirmPending}
	oncancel={() => (pending = null)}
/>

<style>
	section.fed {
		max-width: 46rem;
		margin-bottom: 2.5rem;
	}
	h2 {
		font-size: 1.1rem;
		margin: 0 0 0.75rem;
		padding-bottom: 0.4rem;
		border-bottom: 1px solid var(--border);
	}
	h3 {
		font-size: 1rem;
		margin: 1.75rem 0 0.5rem;
	}
	h4 {
		font-size: 0.85rem;
		margin: 1.1rem 0 0.4rem;
		color: var(--muted);
	}
	.hint {
		margin: 0.4rem 0 0;
		font-size: 0.8rem;
		color: var(--muted);
	}
	.warn {
		margin: 0 0 0.75rem;
		font-size: 0.85rem;
		color: var(--danger);
		border-left: 3px solid var(--danger);
		padding-left: 0.75rem;
	}
	code,
	.mono {
		font-family: ui-monospace, 'SF Mono', monospace;
		font-size: 0.8rem;
		overflow-wrap: anywhere;
	}
	.settings {
		display: flex;
		flex-direction: column;
		gap: 0.9rem;
		max-width: 34rem;
		margin-top: 0.75rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.9rem;
		color: var(--muted);
	}
	label .hint {
		margin: 0;
	}
	label.narrow input {
		max-width: 8rem;
	}
	select {
		font: inherit;
		background: var(--card);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 0.45rem 0.6rem;
	}
	select:focus {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.row input {
		min-width: 0;
	}
	.row button {
		flex-shrink: 0;
	}
	.ok {
		color: var(--accent);
		font-size: 0.85rem;
	}
	.preview {
		margin-top: 0.75rem;
		padding: 0.75rem 1rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
	}
	dl {
		display: grid;
		grid-template-columns: max-content minmax(0, 1fr);
		gap: 0.25rem 1rem;
		margin: 0 0 0.75rem;
		font-size: 0.88rem;
	}
	dt {
		color: var(--muted);
	}
	dd {
		margin: 0;
		overflow-wrap: anywhere;
	}
	.scroll {
		overflow-x: auto;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.88rem;
	}
	table.small {
		font-size: 0.8rem;
	}
	th {
		text-align: left;
		font-size: 0.72rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--muted);
		padding: 0.35rem 0.5rem;
	}
	td {
		padding: 0.45rem 0.5rem;
		border-top: 1px solid var(--border);
		vertical-align: top;
	}
	.num {
		text-align: right;
	}
	.nowrap {
		white-space: nowrap;
	}
	.muted {
		color: var(--muted);
	}
	.domain {
		font-weight: 600;
	}
	.note {
		display: block;
		color: var(--muted);
		font-size: 0.75rem;
	}
	.badge {
		display: inline-block;
		font-size: 0.72rem;
		color: var(--accent);
		border: 1px solid var(--accent);
		border-radius: 99px;
		padding: 0.05rem 0.5rem;
		white-space: nowrap;
	}
	.badge.bad {
		color: var(--danger);
		border-color: var(--danger);
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.35rem;
		white-space: nowrap;
	}
	.danger {
		color: var(--danger);
		border-color: var(--danger);
	}
	.icon {
		padding: 0.28rem 0.4rem;
	}
	.requests {
		list-style: none;
		margin: 0 0 1.5rem;
		padding: 0;
		border: 1px solid var(--accent);
		border-radius: 8px;
		background: var(--card);
	}
	.requests li {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		padding: 0.7rem 0.9rem;
	}
	.requests li + li {
		border-top: 1px solid var(--border);
	}
	.request-info {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		min-width: 0;
	}
	.request-line {
		font-size: 0.85rem;
		overflow-wrap: anywhere;
	}
	.request-actions {
		display: flex;
		gap: 0.4rem;
		flex-shrink: 0;
	}
	.request-actions button {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
	}
</style>
