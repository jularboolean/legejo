<script lang="ts">
	import { Check, Copy } from '@lucide/svelte';
	import { t } from '#lib/i18n';

	const url = typeof location !== 'undefined' ? `${location.origin}/api/mcp` : '/api/mcp';
	let copied = $state(false);

	async function copy() {
		try {
			await navigator.clipboard.writeText(url);
			copied = true;
			setTimeout(() => (copied = false), 1500);
		} catch {
			// The field is selectable instead.
		}
	}
</script>

<section class="mcp">
	<h3>{t('mcp.heading')}</h3>
	<p class="intro">{t('mcp.intro')}</p>
	<div class="row">
		<input readonly value={url} aria-label={t('mcp.url')} onfocus={(e) => e.currentTarget.select()} />
		<button type="button" class="ghost" onclick={copy}>
			{#if copied}<Check size={13} /> {t('kobo.copied')}{:else}<Copy size={13} /> {t('kobo.copy')}{/if}
		</button>
	</div>
	<p class="hint">{t('mcp.hint')}</p>
</section>

<style>
	.mcp {
		margin-top: 2rem;
		max-width: 32rem;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	h3 {
		font-size: 1rem;
		margin: 0;
	}
	.intro,
	.hint {
		margin: 0;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.hint {
		font-size: 0.8rem;
	}
	.row {
		display: flex;
		gap: 0.5rem;
	}
	.row input {
		flex: 1;
		min-width: 0;
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		font-size: 0.85rem;
	}
	.row button {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
	}
</style>
