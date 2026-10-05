<script lang="ts">
	import { page } from '$app/state';
	import Logo from '#lib/Logo.svelte';
	import { t } from '#lib/i18n';

	let password = $state('');
	let busy = $state(false);
	let done = $state(false);
	let error = $state('');

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			const res = await fetch('/api/register/reset', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					token: page.url.searchParams.get('token') ?? '',
					password
				})
			});
			if (res.ok) {
				done = true;
			} else if (res.status === 410) {
				error = t('verify.failed');
			} else {
				const body = await res.json().catch(() => null);
				error = body?.error ?? t('edit.saveFailed');
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
		<h2>{t('reset.heading')}</h2>
		{#if done}
			<p class="done">{t('reset.done')}</p>
			<a class="login" href="/login">{t('verify.login')}</a>
		{:else}
			<label>
				{t('reset.newPassword')}
				<input type="password" bind:value={password} autocomplete="new-password" required minlength="8" />
			</label>
			{#if error}<p class="error">{error}</p>{/if}
			<button disabled={busy}>{busy ? t('edit.saving') : t('reset.submit')}</button>
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
	.login {
		text-align: center;
	}
</style>
