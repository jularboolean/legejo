<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import { nextPage } from '#lib/nextPage';
	import Logo from '#lib/Logo.svelte';
	import { getLocale, LANGUAGES, setLocale, t } from '#lib/i18n';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	let username = $state('');
	let password = $state('');
	// Back from the identity provider with ?oidc=<reason>.
	const oidcErrors = {
		failed: 'login.oidc.failed',
		no_account: 'login.oidc.no_account',
		unavailable: 'login.oidc.unavailable',
		error: 'login.oidc.error'
	} as const;
	const oidcParam = page.url.searchParams.get('oidc') as keyof typeof oidcErrors | null;
	let error = $state(oidcParam && oidcErrors[oidcParam] ? t(oidcErrors[oidcParam]) : '');
	let busy = $state(false);
	// Where to go once logged in: the page that sent us here, if any.
	const next = nextPage(page.url.searchParams.get('next'));
	const ssoHref = $derived(next === '/' ? '/api/auth/oidc/start' : `/api/auth/oidc/start?next=${encodeURIComponent(next)}`);

	// First start: no account yet, so this one becomes the admin. Taken once;
	// the page itself switches to logging in if someone was first.
	// svelte-ignore state_referenced_locally
	let creating = $state(data.setup);
	let repeat = $state('');

	async function createAdmin(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		if (password.length < 8) {
			error = t('account.passwordTooShort');
			return;
		}
		if (password !== repeat) {
			error = t('setup.mismatch');
			return;
		}
		busy = true;
		try {
			const res = await fetch('/api/auth/setup', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ username, password, locale: getLocale() })
			});
			if (res.ok) {
				await invalidateAll();
				goto('/');
			} else if (res.status === 409) {
				// Someone was first: an ordinary login from here.
				creating = false;
				error = t('setup.taken');
			} else {
				error = t('setup.failed');
			}
		} catch {
			error = t('common.network');
		} finally {
			busy = false;
		}
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			const res = await fetch('/api/auth/login', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ username, password })
			});
			if (res.ok) {
				await invalidateAll();
				goto(next);
			} else {
				const body = await res.json().catch(() => null);
				error =
					res.status === 429
						? t('login.tooMany')
						: body?.error === 'email not verified'
							? t('login.unverified')
							: t('login.failed');
			}
		} catch {
			error = t('common.network');
		} finally {
			busy = false;
		}
	}
</script>

<div class="wrap">
	{#if creating}
	<form onsubmit={createAdmin}>
		<h1>
			<Logo size={124} variant="pair" />
			<span class="wordmark">{t('app.name')} <span class="version">{__APP_VERSION__}</span></span>
		</h1>
		<div class="welcome">
			<h2>{t('setup.title')}</h2>
			<p>{t('setup.lead')}</p>
		</div>
		<label>
			{t('setup.language')}
			<select value={getLocale()} onchange={(e) => setLocale(e.currentTarget.value)}>
				{#each LANGUAGES as language (language.code)}
					<option value={language.code}>{language.label}</option>
				{/each}
			</select>
		</label>
		<label>
			{t('login.username')}
			<input bind:value={username} autocomplete="username" required />
		</label>
		<label>
			{t('login.password')}
			<input type="password" bind:value={password} autocomplete="new-password" minlength="8" required />
		</label>
		<label>
			{t('setup.repeat')}
			<input type="password" bind:value={repeat} autocomplete="new-password" minlength="8" required />
		</label>
		{#if error}
			<p class="error">{error}</p>
		{/if}
		<button disabled={busy}>{busy ? t('setup.submitting') : t('setup.submit')}</button>
	</form>
	{:else}
	<form onsubmit={submit}>
		<h1>
			<Logo size={124} variant="pair" />
			<span class="wordmark">{t('app.name')} <span class="version">{__APP_VERSION__}</span></span>
		</h1>
		<label>
			{t('login.username')}
			<input bind:value={username} autocomplete="username" required />
		</label>
		<label>
			{t('login.password')}
			<input type="password" bind:value={password} autocomplete="current-password" required />
		</label>
		{#if error}
			<p class="error">{error}</p>
		{/if}
		<button disabled={busy}>{busy ? t('login.submitting') : t('login.submit')}</button>
		{#if data.oidcName}
			<div class="or"><span>{t('login.or')}</span></div>
			<a class="sso" href={ssoHref} data-sveltekit-reload>{t('login.sso', { name: data.oidcName })}</a>
		{/if}
		{#if data.registrationEnabled}
			<a class="register" href="/register">{t('login.register')}</a>
		{/if}
		{#if data.mailConfigured}
			<a class="register" href="/forgot">{t('login.forgot')}</a>
		{/if}
	</form>
	{/if}
</div>

<style>
	.wrap {
		display: flex;
		justify-content: center;
		padding-top: 10vh;
	}
	form {
		width: 100%;
		max-width: 20rem;
		display: flex;
		flex-direction: column;
		gap: 1rem;
		background: var(--card);
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 2rem;
	}
	/* The mark and the name under it, as at the top of the menu once logged in. */
	h1 {
		margin: 0 0 0.5rem;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.5rem;
		font: inherit;
	}
	.wordmark {
		font-size: 0.8rem;
		font-weight: 500;
		letter-spacing: 0.04em;
		color: var(--muted);
	}
	.version {
		font-variant-numeric: tabular-nums;
	}

	.welcome h2 {
		margin: 0 0 0.35rem;
		font-size: 1.15rem;
		text-align: center;
	}
	.welcome p {
		margin: 0;
		font-size: 0.88rem;
		color: var(--muted);
		text-align: center;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.9rem;
		color: var(--muted);
	}
	.or {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		font-size: 0.8rem;
		color: var(--muted);
	}
	.or::before,
	.or::after {
		content: '';
		flex: 1;
		border-top: 1px solid var(--border);
	}
	.sso {
		display: block;
		text-align: center;
		padding: 0.55rem 0.9rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		text-decoration: none;
		font-size: 0.95rem;
	}
	.sso:hover {
		border-color: var(--accent);
	}
	.register {
		text-align: center;
		font-size: 0.85rem;
	}
</style>
