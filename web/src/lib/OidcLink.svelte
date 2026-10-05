<script lang="ts">
	import { page } from '$app/state';
	import { Check, Link } from '@lucide/svelte';
	import { t } from '#lib/i18n';

	// Rendered only when the instance has OIDC login (LEGEJO_OIDC_*).
	let status = $state<{ name: string; linked: boolean } | null>(null);
	const result = page.url.searchParams.get('oidc');

	$effect(() => {
		fetch('/api/account/oidc')
			.then((res) => (res.ok ? res.json() : null))
			.then((v) => (status = v))
			.catch(() => {});
	});
</script>

{#if status}
	<section class="oidc">
		<h2>{t('oidc.heading', { name: status.name })}</h2>
		{#if result === 'linked'}
			<p class="ok"><Check size={14} /> {t('oidc.justLinked', { name: status.name })}</p>
		{:else if result === 'already_linked'}
			<p class="error">{t('oidc.alreadyLinked', { name: status.name })}</p>
		{:else if result === 'failed' || result === 'unavailable' || result === 'error'}
			<p class="error">{t('oidc.failed')}</p>
		{/if}
		{#if status.linked}
			<p class="intro">{t('oidc.linked', { name: status.name })}</p>
		{:else}
			<p class="intro">{t('oidc.notLinked', { name: status.name })}</p>
			<a class="link" href="/api/auth/oidc/start?link=1" data-sveltekit-reload>
				<Link size={14} />
				{t('oidc.link', { name: status.name })}
			</a>
		{/if}
	</section>
{/if}

<style>
	.oidc {
		margin-top: 2.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--border);
		max-width: 32rem;
	}
	h2 {
		font-size: 1.1rem;
		margin: 0 0 0.5rem;
	}
	.intro {
		margin: 0 0 0.75rem;
		font-size: 0.9rem;
		color: var(--muted);
	}
	.ok {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		margin: 0 0 0.6rem;
		font-size: 0.9rem;
		color: var(--accent);
	}
	.error {
		margin: 0 0 0.6rem;
	}
	.link {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.4rem 0.8rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		text-decoration: none;
		font-size: 0.9rem;
	}
	.link:hover {
		border-color: var(--accent);
	}
</style>
