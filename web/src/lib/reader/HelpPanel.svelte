<script lang="ts">
	import { t } from '#lib/i18n';
	import type { MessageKey } from '#lib/i18n/en';
	import Sheet from './Sheet.svelte';

	type Props = {
		open: boolean;
		/** Leave out the fullscreen shortcut where the browser can't do it. */
		fullscreen: boolean;
		onclose: () => void;
	};
	let { open, fullscreen, onclose }: Props = $props();

	type Shortcut = { keys: string[]; label: MessageKey };

	const shortcuts = $derived<Shortcut[]>([
		{ keys: ['→', 'Space', 'PgDn'], label: 'reader.help.next' },
		{ keys: ['←', '⇧ Space', 'PgUp'], label: 'reader.help.prev' },
		{ keys: ['T'], label: 'reader.help.toc' },
		{ keys: ['/'], label: 'reader.help.search' },
		{ keys: ['S'], label: 'reader.help.settings' },
		{ keys: ['+', '−'], label: 'reader.help.size' },
		...(fullscreen ? [{ keys: ['F'], label: 'reader.help.fullscreen' } as Shortcut] : []),
		{ keys: ['Esc'], label: 'reader.help.escape' },
		{ keys: ['?'], label: 'reader.help.help' }
	]);
</script>

<Sheet {open} title={t('reader.help')} side="center" {onclose}>
	<dl>
		{#each shortcuts as shortcut (shortcut.label)}
			<div>
				<dt>{t(shortcut.label)}</dt>
				<dd>
					{#each shortcut.keys as key (key)}<kbd>{key}</kbd>{/each}
				</dd>
			</div>
		{/each}
	</dl>
	<p>{t('reader.help.pointer')}</p>
</Sheet>

<style>
	dl {
		margin: 0;
		padding: 0.5rem 1.25rem 0;
	}
	dl div {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.45rem 0;
		border-bottom: 1px solid color-mix(in srgb, var(--r-border) 60%, transparent);
	}
	dt {
		min-width: 0;
	}
	dd {
		display: flex;
		gap: 0.3rem;
		margin: 0;
		flex-shrink: 0;
	}
	p {
		margin: 0;
		padding: 0.9rem 1.25rem 1.1rem;
		font-size: 0.85rem;
		color: var(--r-muted);
	}
</style>
