<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { Check, Copy, UserPlus } from '@lucide/svelte';
	import { t } from '#lib/i18n';

	let { mailConfigured }: { mailConfigured: boolean } = $props();

	let username = $state('');
	let email = $state('');
	let busy = $state(false);
	let errorMsg = $state('');
	let result = $state<{ username: string; link: string; mailed: boolean; mail_error: string | null } | null>(null);
	let copied = $state(false);

	const ERRORS: Record<string, Parameters<typeof t>[0]> = {
		'username must be 1-60 characters': 'invite.error.username',
		'that does not look like an email address': 'invite.error.email',
		'username or email is already in use': 'invite.error.taken'
	};

	async function invite(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		errorMsg = '';
		result = null;
		copied = false;
		try {
			const res = await fetch('/api/admin/invites', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ username: username.trim(), email: email.trim() || null })
			});
			const body = await res.json().catch(() => null);
			if (!res.ok) {
				const key = ERRORS[body?.error];
				errorMsg = key ? t(key) : t('edit.saveFailed');
				return;
			}
			result = body;
			username = '';
			email = '';
			await invalidateAll();
		} catch {
			errorMsg = t('common.network');
		} finally {
			busy = false;
		}
	}

	async function copy() {
		if (!result) return;
		try {
			await navigator.clipboard.writeText(result.link);
			copied = true;
		} catch {
			// Clipboard unavailable; the field is selectable instead.
		}
	}
</script>

<form class="invite" onsubmit={invite}>
	<h3>{t('invite.heading')}</h3>
	<p class="hint">{mailConfigured ? t('invite.hintMail') : t('invite.hintNoMail')}</p>
	<div class="fields">
		<input bind:value={username} placeholder={t('invite.username')} aria-label={t('invite.username')} required maxlength="60" />
		<input bind:value={email} type="email" placeholder={t('invite.email')} aria-label={t('invite.email')} />
		<button disabled={busy || !username.trim()}><UserPlus size={13} /> {t('invite.submit')}</button>
	</div>
	{#if errorMsg}<p class="error">{errorMsg}</p>{/if}
	{#if result}
		<div class="result">
			<p>
				{#if result.mailed}
					{t('invite.mailed', { username: result.username })}
				{:else}
					{t('invite.copyLink', { username: result.username })}
				{/if}
			</p>
			{#if result.mail_error}<p class="error">{t('invite.mailFailed')}</p>{/if}
			<div class="link-row">
				<input readonly value={result.link} onfocus={(e) => e.currentTarget.select()} />
				<button type="button" class="ghost" onclick={copy}>
					{#if copied}<Check size={13} /> {t('kobo.copied')}{:else}<Copy size={13} /> {t('kobo.copy')}{/if}
				</button>
			</div>
			<p class="hint">{t('invite.validity')}</p>
		</div>
	{/if}
</form>

<style>
	.invite {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		margin-bottom: 1.5rem;
	}
	h3 {
		margin: 0;
		font-size: 1rem;
	}
	.hint {
		margin: 0;
		font-size: 0.8rem;
		color: var(--muted);
	}
	.fields {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
	}
	.fields input {
		flex: 1 1 10rem;
		width: auto;
	}
	.fields button {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
	}
	.result {
		background: var(--card);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 0.75rem 0.9rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.result p {
		margin: 0;
		font-size: 0.9rem;
	}
	.link-row {
		display: flex;
		gap: 0.5rem;
	}
	.link-row input {
		font-size: 0.8rem;
	}
	.link-row button {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
	}
</style>
