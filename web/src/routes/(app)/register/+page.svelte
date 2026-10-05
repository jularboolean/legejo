<script lang="ts">
	import Logo from '#lib/Logo.svelte';
	import { t } from '#lib/i18n';

	let username = $state('');
	let email = $state('');
	let password = $state('');
	let busy = $state(false);
	let done = $state(false);
	let error = $state('');

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			const res = await fetch('/api/register', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ username, email, password })
			});
			if (res.ok) {
				done = true;
			} else {
				const body = await res.json().catch(() => null);
				error = body?.error ?? t('register.failed');
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
		<h2>{t('register.heading')}</h2>
		{#if done}
			<p class="done">{t('register.done')}</p>
		{:else}
			<label>
				{t('login.username')}
				<input bind:value={username} autocomplete="username" required maxlength="60" />
			</label>
			<label>
				{t('register.email')}
				<input type="email" bind:value={email} autocomplete="email" required />
			</label>
			<label>
				{t('login.password')}
				<input type="password" bind:value={password} autocomplete="new-password" required minlength="8" />
			</label>
			{#if error}<p class="error">{error}</p>{/if}
			<button disabled={busy}>{busy ? t('register.submitting') : t('register.submit')}</button>
		{/if}
		<a class="back" href="/login">{t('register.backToLogin')}</a>
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
		font-size: 1.15rem;
		text-align: center;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.9rem;
		color: var(--muted);
	}
	.done {
		margin: 0;
		color: var(--accent);
		font-size: 0.95rem;
	}
	.back {
		text-align: center;
		font-size: 0.85rem;
		color: var(--muted);
	}
</style>
