<script lang="ts">
	import { Minus, Plus, RotateCcw, TextAlignJustify, TextAlignStart } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import Sheet from './Sheet.svelte';
	import { ALIGNS, defaultSettings, FLOWS, FONTS, LIMITS, SPREADS, THEMES } from './settings';
	import { fontStacks, palettes } from './themes';
	import type { ReaderSettings } from './types';

	type Props = {
		open: boolean;
		settings: ReaderSettings;
		/** Called with a complete new settings object on every change. */
		onchange: (settings: ReaderSettings) => void;
		onclose: () => void;
	};
	let { open, settings, onchange, onclose }: Props = $props();

	type Numeric = 'fontSize' | 'lineHeight' | 'margin';

	function set<K extends keyof ReaderSettings>(key: K, value: ReaderSettings[K]) {
		if (settings[key] !== value) onchange({ ...settings, [key]: value });
	}

	function step(key: Numeric, direction: 1 | -1) {
		const { min, max, step } = LIMITS[key];
		// Rounded, or 1.5 + 0.1 becomes 1.6000000000000001.
		const next = Math.round((settings[key] + direction * step) * 10) / 10;
		set(key, Math.min(max, Math.max(min, next)));
	}

	const atMin = (key: Numeric) => settings[key] <= LIMITS[key].min;
	const atMax = (key: Numeric) => settings[key] >= LIMITS[key].max;

	const defaults = defaultSettings();
	const isDefault = $derived(
		(Object.keys(defaults) as (keyof ReaderSettings)[]).every(
			// The theme isn't a deviation to undo; it follows taste and time of day.
			(key) => key === 'theme' || settings[key] === defaults[key]
		)
	);

	function reset() {
		onchange({ ...defaultSettings(), theme: settings.theme });
	}

	/** Arrow keys move within a radio group, as they do for native radios. */
	function radioKeys(event: KeyboardEvent) {
		const keys = ['ArrowLeft', 'ArrowUp', 'ArrowRight', 'ArrowDown'];
		const at = keys.indexOf(event.key);
		if (at < 0) return;
		const group = event.currentTarget as HTMLElement;
		const radios = [...group.querySelectorAll<HTMLButtonElement>('[role="radio"]')];
		const current = radios.indexOf(document.activeElement as HTMLButtonElement);
		if (current < 0) return;
		event.preventDefault();
		const next = radios[(current + (at < 2 ? -1 : 1) + radios.length) % radios.length];
		next.focus();
		next.click();
	}
</script>

{#snippet stepper(key: Numeric, label: string, value: string, less: string, more: string)}
	<div class="stepper" role="group" aria-label={label}>
		<button
			class="r-icon"
			onclick={() => step(key, -1)}
			disabled={atMin(key)}
			aria-label={less}
			title={less}
		>
			<Minus size={18} />
		</button>
		<output aria-live="polite">{value}</output>
		<button
			class="r-icon"
			onclick={() => step(key, 1)}
			disabled={atMax(key)}
			aria-label={more}
			title={more}
		>
			<Plus size={18} />
		</button>
	</div>
{/snippet}

<Sheet {open} title={t('reader.settings')} side="right" preview {onclose}>
	{#snippet actions()}
		<button
			class="r-icon"
			onclick={reset}
			disabled={isDefault}
			aria-label={t('reader.reset')}
			title={t('reader.reset')}
		>
			<RotateCcw size={18} />
		</button>
	{/snippet}

	<div class="settings">
		<div class="themes" role="radiogroup" tabindex="-1" aria-label={t('reader.theme')} onkeydown={radioKeys}>
			{#each THEMES as theme (theme)}
				<button
					class="swatch"
					role="radio"
					aria-checked={settings.theme === theme}
					tabindex={settings.theme === theme ? 0 : -1}
					onclick={() => set('theme', theme)}
				>
					<span
						class="sample"
						style:background={palettes[theme].bg}
						style:color={palettes[theme].fg}
						style:border-color={palettes[theme].border}
						aria-hidden="true"
					>
						Aa
					</span>
					{t(`reader.theme.${theme}`)}
				</button>
			{/each}
		</div>

		<div class="row">
			<h3 id="rs-size">{t('reader.fontSize')}</h3>
			<div class="size" role="group" aria-labelledby="rs-size">
				<button
					class="size-step small"
					onclick={() => step('fontSize', -1)}
					disabled={atMin('fontSize')}
					aria-label={t('reader.smaller')}
					title={t('reader.smaller')}
				>
					A<span aria-hidden="true">−</span>
				</button>
				<button
					class="size-value"
					onclick={() => set('fontSize', 100)}
					title={t('reader.fontSizeReset')}
					aria-label={t('reader.fontSizeValue', { percent: settings.fontSize })}
				>
					<output aria-live="polite">{settings.fontSize} %</output>
				</button>
				<button
					class="size-step large"
					onclick={() => step('fontSize', 1)}
					disabled={atMax('fontSize')}
					aria-label={t('reader.larger')}
					title={t('reader.larger')}
				>
					A<span aria-hidden="true">+</span>
				</button>
			</div>
		</div>

		<div class="row">
			<h3 id="rs-font">{t('reader.font')}</h3>
			<div class="r-segments" role="radiogroup" tabindex="-1" aria-labelledby="rs-font" onkeydown={radioKeys}>
				{#each FONTS as font (font)}
					<button
						role="radio"
						aria-checked={settings.fontFamily === font}
						tabindex={settings.fontFamily === font ? 0 : -1}
						style:font-family={font === 'book' ? undefined : fontStacks[font]}
						onclick={() => set('fontFamily', font)}
					>
						{t(`reader.font.${font}`)}
					</button>
				{/each}
			</div>
		</div>

		<div class="pair">
			<div class="row">
				<h3>{t('reader.lineHeight')}</h3>
				{@render stepper(
					'lineHeight',
					t('reader.lineHeight'),
					settings.lineHeight.toFixed(1),
					t('reader.lineHeight.less'),
					t('reader.lineHeight.more')
				)}
			</div>
			<div class="row">
				<h3>{t('reader.margin')}</h3>
				{@render stepper(
					'margin',
					t('reader.margin'),
					`${settings.margin} %`,
					t('reader.margin.less'),
					t('reader.margin.more')
				)}
			</div>
		</div>

		<div class="row">
			<h3 id="rs-align">{t('reader.align')}</h3>
			<div class="r-segments" role="radiogroup" tabindex="-1" aria-labelledby="rs-align" onkeydown={radioKeys}>
				{#each ALIGNS as align (align)}
					<button
						role="radio"
						aria-checked={settings.textAlign === align}
						tabindex={settings.textAlign === align ? 0 : -1}
						onclick={() => set('textAlign', align)}
					>
						{#if align === 'left'}<TextAlignStart size={16} />{/if}
						{#if align === 'justify'}<TextAlignJustify size={16} />{/if}
						{t(`reader.align.${align}`)}
					</button>
				{/each}
			</div>
		</div>

		<label class="switch-row">
			<span>
				{t('reader.hyphenate')}
				<small>{t('reader.hyphenate.hint')}</small>
			</span>
			<input
				type="checkbox"
				role="switch"
				checked={settings.hyphenate}
				onchange={(e) => set('hyphenate', e.currentTarget.checked)}
			/>
		</label>

		<div class="row">
			<h3 id="rs-flow">{t('reader.flow')}</h3>
			<div class="r-segments" role="radiogroup" tabindex="-1" aria-labelledby="rs-flow" onkeydown={radioKeys}>
				{#each FLOWS as flow (flow)}
					<button
						role="radio"
						aria-checked={settings.flow === flow}
						tabindex={settings.flow === flow ? 0 : -1}
						onclick={() => set('flow', flow)}
					>
						{t(`reader.flow.${flow}`)}
					</button>
				{/each}
			</div>
		</div>

		<!-- Two pages side by side only happen on wide screens; no point offering
		     the choice on a phone. -->
		{#if settings.flow === 'paginated'}
			<div class="row wide-only">
				<h3 id="rs-spread">{t('reader.spread')}</h3>
				<div class="r-segments" role="radiogroup" tabindex="-1" aria-labelledby="rs-spread" onkeydown={radioKeys}>
					{#each SPREADS as spread (spread)}
						<button
							role="radio"
							aria-checked={settings.spread === spread}
							tabindex={settings.spread === spread ? 0 : -1}
							onclick={() => set('spread', spread)}
						>
							{t(`reader.spread.${spread}`)}
						</button>
					{/each}
				</div>
			</div>
		{/if}
	</div>
</Sheet>

<style>
	.settings {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		padding: 1rem 1.25rem 1.25rem;
	}
	.row {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		min-width: 0;
	}
	.pair {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.75rem;
	}
	h3 {
		margin: 0;
		font-family: inherit;
		font-size: 0.72rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--r-muted);
	}

	.themes {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 0.5rem;
	}
	.swatch {
		flex-direction: column;
		gap: 0.35rem;
		padding: 0.3rem 0.3rem 0.4rem;
		border-radius: 14px;
		font-size: 0.8rem;
		color: var(--r-muted);
	}
	.sample {
		display: grid;
		place-items: center;
		width: 100%;
		height: 2.9rem;
		border: 1px solid;
		border-radius: 10px;
		font-family: Georgia, 'Times New Roman', serif;
		font-size: 1.25rem;
		font-weight: 400;
	}
	.swatch[aria-checked='true'] {
		color: var(--r-fg);
		font-weight: 600;
	}
	.swatch[aria-checked='true'] .sample {
		box-shadow:
			0 0 0 2px var(--r-surface),
			0 0 0 4px var(--r-link);
	}

	.size,
	.stepper {
		display: flex;
		align-items: center;
		border-radius: 12px;
		background: color-mix(in srgb, var(--r-fg) 7%, transparent);
	}
	.stepper {
		justify-content: space-between;
	}
	.stepper output,
	.size output {
		font-variant-numeric: tabular-nums;
		font-weight: 600;
	}
	.size-step {
		flex: 1;
		justify-content: center;
		gap: 0.1rem;
		min-height: 2.9rem;
		font-family: Georgia, 'Times New Roman', serif;
		border-radius: 12px;
	}
	.size-step.small {
		font-size: 1rem;
	}
	.size-step.large {
		font-size: 1.45rem;
	}
	.size-step span {
		font-family: system-ui, sans-serif;
		font-size: 0.8em;
	}
	.size-value {
		flex: 0 0 5rem;
		justify-content: center;
		min-height: 2.9rem;
		border-left: 1px solid var(--r-border);
		border-right: 1px solid var(--r-border);
		border-radius: 0;
	}
	.size-step:active:not(:disabled) {
		background: color-mix(in srgb, var(--r-fg) 10%, transparent);
	}

	.switch-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		min-height: 2.75rem;
		cursor: pointer;
	}
	.switch-row span {
		font-weight: 500;
	}
	.switch-row small {
		display: block;
		font-weight: 400;
		font-size: 0.78rem;
		color: var(--r-muted);
	}
	/* A switch drawn from a checkbox. */
	.switch-row input {
		appearance: none;
		-webkit-appearance: none;
		flex-shrink: 0;
		position: relative;
		width: 3rem;
		height: 1.75rem;
		margin: 0;
		padding: 0;
		border: none;
		border-radius: 99px;
		background: color-mix(in srgb, var(--r-fg) 22%, transparent);
		cursor: pointer;
		transition: background 0.15s ease;
	}
	.switch-row input::after {
		content: '';
		position: absolute;
		top: 0.2rem;
		left: 0.2rem;
		width: 1.35rem;
		height: 1.35rem;
		border-radius: 50%;
		background: #fff;
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
		transition: transform 0.15s ease;
	}
	.switch-row input:checked {
		background: var(--r-link);
	}
	.switch-row input:checked::after {
		transform: translateX(1.25rem);
	}

	@media (max-width: 62rem) {
		.wide-only {
			display: none;
		}
	}
	@media (max-width: 40rem) {
		.settings {
			gap: 0.8rem;
			padding-top: 0.8rem;
		}
	}
</style>
