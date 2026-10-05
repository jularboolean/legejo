<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import Logo from '#lib/Logo.svelte';
	import { t } from '#lib/i18n';

	const token = page.url.searchParams.get('token') ?? '';
	let username = $state<string | null>(null);
	let invalid = $state(false);
	let password = $state('');
	let busy = $state(false);
	let error = $state('');

	$effect(() => {
		fetch(`/api/invite?token=${encodeURIComponent(token)}`)
			.then(async (res) => {
				if (res.ok) username = (await res.json()).username;
				else invalid = true;
			})
			.catch(() => (error = t('common.network')));
	});

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			const res = await fetch('/api/invite', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ token, password })
			});
			if (res.ok) {
				// Logged in by the server: straight into the library.
				await invalidateAll();
				goto('/');
			} else if (res.status === 410) {
				invalid = true;
			} else {
				const body = await res.json().catch(() => null);
				error = body?.error === 'password must be at least 8 characters' ? t('invite.passwordShort') : t('edit.saveFailed');
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
		<h1 aria-label={t('app.name')}><Logo size={84} /></h1>
		{#if invalid}
			<h2>{t('invite.invalidHeading')}</h2>
			<p class="muted">{t('invite.invalid')}</p>
			<a class="login" href="/login">{t('verify.login')}</a>
		{:else if username}
			<h2>{t('invite.welcome', { username })}</h2>
			<p class="muted">{t('invite.choosePassword')}</p>
			<label>
				{t('invite.yourUsername')}
				<input value={username} readonly autocomplete="username" />
			</label>
			<label>
				{t('reset.newPassword')}
				<input type="password" bind:value={password} autocomplete="new-password" required minlength="8" />
			</label>
			{#if error}<p class="error">{error}</p>{/if}
			<button disabled={busy}>{busy ? t('edit.saving') : t('invite.activate')}</button>
		{:else if error}
			<p class="error">{error}</p>
		{/if}
	</form>
</div>

<style>
	.wrap {
		display: flex;
		justify-content: center;
		padding-top: 8vh;
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
		margin: 0;
		display: flex;
		justify-content: center;
	}
	h2 {
		margin: 0;
		font-size: 1.1rem;
		text-align: center;
	}
	.muted {
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
	input[readonly] {
		opacity: 0.8;
	}
	.login {
		text-align: center;
	}
</style>
