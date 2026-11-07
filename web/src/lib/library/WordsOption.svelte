<script lang="ts">
	// The vocabulary option, offered where it is needed: on My words and a
	// book's study list while it is off (it lives under Account → Options).
	import { invalidateAll } from '$app/navigation';
	import { t } from '#lib/i18n';

	let error = $state('');
	async function turnOn() {
		error = '';
		try {
			const res = await fetch('/api/account/words', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ enabled: true })
			});
			if (res.ok) await invalidateAll();
			else error = t('edit.saveFailed');
		} catch {
			error = t('common.network');
		}
	}
</script>

<section class="option">
	<h2>{t('words.option.heading')}</h2>
	<p>{t('words.option.intro')}</p>
	<label><input type="checkbox" onchange={turnOn} /> {t('words.option.turnOn')}</label>
	{#if error}<p class="error">{error}</p>{/if}
</section>

<style>
	.option {
		max-width: 36rem;
		padding: 0.9rem 1rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
	}
	h2 {
		font-size: 1.05rem;
		margin: 0 0 0.4rem;
	}
	p {
		color: var(--muted);
		margin: 0 0 0.7rem;
	}
</style>
