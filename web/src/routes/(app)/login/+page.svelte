<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import Logo from '#lib/Logo.svelte';
	import { t } from '#lib/i18n';
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
				goto('/');
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
	<form onsubmit={submit}>
		<h1 aria-label={t('app.name')}><Logo size={104} /></h1>
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
			<a class="sso" href="/api/auth/oidc/start" data-sveltekit-reload>{t('login.sso', { name: data.oidcName })}</a>
		{/if}
		{#if data.registrationEnabled}
			<a class="register" href="/register">{t('login.register')}</a>
		{/if}
		{#if data.mailConfigured}
			<a class="register" href="/forgot">{t('login.forgot')}</a>
		{/if}
	</form>
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
	h1 {
		margin: 0 0 0.5rem;
		display: flex;
		justify-content: center;
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
