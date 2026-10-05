<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import { Copy, ImageOff, ImagePlus, RefreshCw, Trash2, Check } from '@lucide/svelte';
	import AccountLeave from '#lib/AccountLeave.svelte';
	import AppPasswords from '#lib/AppPasswords.svelte';
	import OidcLink from '#lib/OidcLink.svelte';
	import KosyncSetup from '#lib/KosyncSetup.svelte';
	import McpSetup from '#lib/McpSetup.svelte';
	import Avatar from '#lib/Avatar.svelte';
	import { getLocale, LANGUAGES, setLocale, t } from '#lib/i18n';
	import type { Account } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	let account = $state<Account>({ ...data.account });
	let form = $state({
		username: data.account.username,
		new_password: '',
		current_password: ''
	});
	let saving = $state(false);
	let saved = $state(false);
	let errorMsg = $state('');
	let koboErrorMsg = $state('');
	let copied = $state(false);

	const koboUrl = $derived(
		account.kobo_token ? `${location.origin}/api/kobo/${account.kobo_token}` : ''
	);

	async function save(e: SubmitEvent) {
		e.preventDefault();
		saving = true;
		saved = false;
		errorMsg = '';
		try {
			const res = await fetch('/api/account', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					username: form.username,
					new_password: form.new_password || null,
					current_password: form.current_password
				})
			});
			if (res.ok) {
				account = await res.json();
				form.new_password = '';
				form.current_password = '';
				saved = true;
				await invalidateAll();
			} else {
				const body = await res.json().catch(() => null);
				if (res.status === 403) errorMsg = t('account.wrongPassword');
				else if (res.status === 409) errorMsg = t('account.usernameTaken');
				else if (body?.error?.includes('8 characters')) errorMsg = t('account.passwordTooShort');
				else errorMsg = t('edit.saveFailed');
			}
		} catch {
			errorMsg = t('common.network');
		} finally {
			saving = false;
		}
	}

	let avatarVersion = $state(0);
	let avatarInput = $state<HTMLInputElement>();
	let avatarError = $state('');

	async function uploadAvatar(files: FileList | null) {
		if (!files || files.length === 0) return;
		avatarError = '';
		const body = new FormData();
		body.append('avatar', files[0]);
		try {
			const res = await fetch('/api/account/avatar', { method: 'POST', body });
			if (!res.ok) {
				const err = await res.json().catch(() => null);
				avatarError = err?.error ?? t('account.avatarFailed');
				return;
			}
			avatarVersion++;
			await invalidateAll();
		} catch {
			avatarError = t('common.network');
		} finally {
			if (avatarInput) avatarInput.value = '';
		}
	}

	async function removeAvatar() {
		avatarError = '';
		const res = await fetch('/api/account/avatar', { method: 'DELETE' });
		if (res.ok) await invalidateAll();
	}

	let locale = $state(getLocale());
	let localeError = $state('');

	async function changeLocale(code: string) {
		const previous = locale;
		locale = code;
		setLocale(code); // applies to the whole UI immediately
		localeError = '';
		try {
			const res = await fetch('/api/account/locale', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ locale: code })
			});
			if (!res.ok) {
				locale = previous;
				setLocale(previous);
				localeError = t('edit.saveFailed');
			}
		} catch {
			locale = previous;
			setLocale(previous);
			localeError = t('common.network');
		}
	}

	async function koboRequest(method: 'POST' | 'DELETE') {
		koboErrorMsg = '';
		try {
			const res = await fetch('/api/account/kobo-token', { method });
			if (res.ok) {
				account = await res.json();
				copied = false;
			} else {
				koboErrorMsg = t('edit.saveFailed');
			}
		} catch {
			koboErrorMsg = t('common.network');
		}
	}

	function generateToken() {
		if (account.kobo_token && !confirm(t('kobo.regenerateConfirm'))) return;
		koboRequest('POST');
	}

	function removeToken() {
		if (!confirm(t('kobo.removeConfirm'))) return;
		koboRequest('DELETE');
	}

	const opdsUrl = `${location.origin}/api/opds`;
	let opdsCopied = $state(false);

	async function copyOpdsUrl() {
		try {
			await navigator.clipboard.writeText(opdsUrl);
			opdsCopied = true;
			setTimeout(() => (opdsCopied = false), 2000);
		} catch {
			// Clipboard unavailable; the field is selectable instead.
		}
	}

	async function copyUrl() {
		try {
			await navigator.clipboard.writeText(koboUrl);
			copied = true;
			setTimeout(() => (copied = false), 2000);
		} catch {
			// Clipboard unavailable; the field is selectable instead.
		}
	}
</script>

<h1>{t('account.heading')}</h1>

<form onsubmit={save}>
	<label>
		{t('account.username')}
		<input bind:value={form.username} autocomplete="username" required />
	</label>
	<label>
		{t('account.newPassword')}
		<input
			type="password"
			bind:value={form.new_password}
			autocomplete="new-password"
			minlength="8"
		/>
		<span class="hint">{t('account.newPasswordHint')}</span>
	</label>
	<label>
		{t('account.currentPassword')}
		<input
			type="password"
			bind:value={form.current_password}
			autocomplete="current-password"
			required
		/>
		<span class="hint">{t('account.currentPasswordHint')}</span>
	</label>

	{#if errorMsg}<p class="error">{errorMsg}</p>{/if}
	{#if saved}<p class="saved">{t('account.saved')}</p>{/if}

	<div class="actions">
		<button type="submit" disabled={saving}>{saving ? t('edit.saving') : t('edit.save')}</button>
	</div>
</form>

<section class="avatar-section">
	<h2>{t('account.avatar')}</h2>
	<div class="avatar-row">
		<Avatar
			userId={page.data.user.id}
			hasAvatar={page.data.user.has_avatar}
			size={72}
			version={avatarVersion}
			alt=""
		/>
		<div class="avatar-actions">
			<button type="button" class="ghost" onclick={() => avatarInput?.click()}>
				<ImagePlus size={13} />
				{t('account.avatarChoose')}
			</button>
			{#if page.data.user.has_avatar}
				<button type="button" class="ghost" onclick={removeAvatar}>
					<ImageOff size={13} />
					{t('account.avatarRemove')}
				</button>
			{/if}
		</div>
		<input
			type="file"
			accept="image/*"
			hidden
			bind:this={avatarInput}
			onchange={(e) => uploadAvatar(e.currentTarget.files)}
		/>
	</div>
	{#if avatarError}<p class="error">{avatarError}</p>{/if}
</section>

<section class="language">
	<h2>{t('account.language')}</h2>
	<select value={locale} onchange={(e) => changeLocale(e.currentTarget.value)}>
		{#each LANGUAGES as lang (lang.code)}
			<option value={lang.code}>{lang.label}</option>
		{/each}
	</select>
	<p class="hint">{t('account.languageHint')}</p>
	{#if localeError}<p class="error">{localeError}</p>{/if}
</section>

<OidcLink />

<section class="devices">
	<h2>{t('devices.heading')}</h2>
	<p class="intro">{t('devices.intro')}</p>
</section>

<section class="kobo">
	<h3>{t('opds.heading')}</h3>
	<p class="intro">{t('opds.intro')}</p>
	<div class="token-row">
		<input readonly value={opdsUrl} onfocus={(e) => e.currentTarget.select()} />
		<button type="button" class="ghost" onclick={copyOpdsUrl}>
			{#if opdsCopied}<Check size={13} /> {t('kobo.copied')}{:else}<Copy size={13} /> {t('kobo.copy')}{/if}
		</button>
	</div>
	<p class="intro login-hint">{t('opds.login', { username: account.username })}</p>
	<AppPasswords />
</section>

<section class="kobo">
	<h3>{t('kobo.heading')}</h3>
	<p class="intro">{t('kobo.intro')}</p>

	{#if account.kobo_token}
		<div class="token-row">
			<input readonly value={koboUrl} onfocus={(e) => e.currentTarget.select()} />
			<button type="button" class="ghost" onclick={copyUrl}>
				{#if copied}<Check size={13} /> {t('kobo.copied')}{:else}<Copy size={13} /> {t('kobo.copy')}{/if}
			</button>
		</div>
		<div class="token-actions">
			<button type="button" class="ghost" onclick={generateToken}>
				<RefreshCw size={13} />
				{t('kobo.regenerate')}
			</button>
			<button type="button" class="ghost danger" onclick={removeToken}>
				<Trash2 size={13} />
				{t('kobo.remove')}
			</button>
		</div>
	{:else}
		<button type="button" onclick={generateToken}>{t('kobo.generate')}</button>
	{/if}

	{#if koboErrorMsg}<p class="error">{koboErrorMsg}</p>{/if}
</section>

<KosyncSetup username={account.username} />
{#if data.mcpEnabled}<McpSetup />{/if}

<AccountLeave />

<style>
	.devices {
		margin-top: 2.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--border);
		max-width: 32rem;
	}
	.devices + .kobo {
		border-top: none;
		margin-top: 1rem;
		padding-top: 0;
	}
	h3 {
		font-size: 1rem;
		margin: 0 0 0.5rem;
	}
	h1 {
		font-size: 1.4rem;
		margin: 0 0 1.25rem;
	}
	form {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		max-width: 24rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.9rem;
		color: var(--muted);
	}
	.hint {
		font-size: 0.75rem;
	}
	.saved {
		color: var(--accent);
		font-size: 0.9rem;
		margin: 0;
	}
	.avatar-section {
		margin-top: 2.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--border);
		max-width: 24rem;
	}
	.avatar-row {
		display: flex;
		align-items: center;
		gap: 1rem;
	}
	.avatar-actions {
		display: flex;
		flex-direction: column;
		align-items: start;
		gap: 0.5rem;
	}
	.language {
		margin-top: 2.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--border);
		max-width: 24rem;
	}
	.language select {
		font: inherit;
		background: var(--card);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 0.45rem 0.7rem;
		width: 100%;
	}
	.language .hint {
		margin-top: 0.4rem;
	}
	.login-hint {
		margin: 0;
	}
	.kobo {
		margin-top: 2.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--border);
		max-width: 34rem;
	}
	h2 {
		font-size: 1.1rem;
		margin: 0 0 0.5rem;
	}
	.intro {
		color: var(--muted);
		font-size: 0.9rem;
		margin: 0 0 1rem;
	}
	.token-row {
		display: flex;
		gap: 0.5rem;
		margin-bottom: 0.75rem;
	}
	.token-row input {
		font-size: 0.82rem;
		font-family: ui-monospace, 'SF Mono', monospace;
		color: var(--muted);
	}
	.token-row button {
		flex-shrink: 0;
	}
	.token-actions {
		display: flex;
		gap: 0.5rem;
	}
	.danger {
		color: var(--danger);
		border-color: var(--danger);
	}
</style>
