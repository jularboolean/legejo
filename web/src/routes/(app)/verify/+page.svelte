<script lang="ts">
	import { Check, CircleAlert } from '@lucide/svelte';
	import Logo from '#lib/Logo.svelte';
	import { t } from '#lib/i18n';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();
</script>

<div class="wrap">
	<div class="card">
		<h1 aria-label={t('app.name')}><Logo size={84} /></h1>
		{#if data.ok}
			<p class="done"><Check size={16} /> {t('verify.done', { username: data.username })}</p>
			<a class="login" href="/login">{t('verify.login')}</a>
		{:else}
			<p class="failed"><CircleAlert size={16} /> {t('verify.failed')}</p>
			<a class="login" href="/register">{t('register.heading')}</a>
		{/if}
	</div>
</div>

<style>
	.wrap {
		display: flex;
		justify-content: center;
		padding-top: 10vh;
	}
	.card {
		width: 100%;
		max-width: 22rem;
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
		background: var(--card);
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 2rem;
		text-align: center;
	}
	h1 {
		margin: 0;
		display: flex;
		justify-content: center;
	}
	.done,
	.failed {
		margin: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.4rem;
	}
	.done {
		color: var(--accent);
	}
	.failed {
		color: var(--danger);
	}
</style>
