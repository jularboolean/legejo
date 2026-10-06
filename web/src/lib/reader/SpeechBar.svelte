<script lang="ts">
	import { ChevronLeft, ChevronRight, Minus, Pause, Play, Plus, X } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import { RATE, type SpeechPrefs, type SpeechState } from './speech';

	type Props = {
		state: SpeechState;
		prefs: SpeechPrefs;
		/** The language being read ("sv"); null when the book does not say. */
		language: string | null;
		/** The device's voices for that language. */
		voices: SpeechSynthesisVoice[];
		/** Sits higher while the progress bar is expanded. */
		raised: boolean;
		ontoggle: () => void;
		onstep: (direction: 1 | -1) => void;
		onprefs: (prefs: SpeechPrefs) => void;
		onstop: () => void;
	};
	let { state, prefs, language, voices, raised, ontoggle, onstep, onprefs, onstop }: Props = $props();

	const chosen = $derived(
		voices.find((v) => language !== null && v.voiceURI === prefs.voices[language])?.voiceURI ?? voices[0]?.voiceURI ?? ''
	);

	function rate(direction: 1 | -1) {
		const next = Math.round((prefs.rate + direction * RATE.step) * 10) / 10;
		onprefs({ ...prefs, rate: Math.min(RATE.max, Math.max(RATE.min, next)) });
	}

	function pick(uri: string) {
		if (language === null) return;
		onprefs({ ...prefs, voices: { ...prefs.voices, [language]: uri } });
	}
</script>

<div class="speech" class:raised role="group" aria-label={t('reader.speech')}>
	<button class="r-icon" onclick={() => onstep(-1)} aria-label={t('reader.speech.prev')} title={t('reader.speech.prev')}>
		<ChevronLeft size={20} />
	</button>
	<button
		class="r-icon main"
		onclick={ontoggle}
		aria-label={state === 'playing' ? t('reader.speech.pause') : t('reader.speech.play')}
		title={`${state === 'playing' ? t('reader.speech.pause') : t('reader.speech.play')} (L)`}
	>
		{#if state === 'playing'}<Pause size={20} />{:else}<Play size={20} />{/if}
	</button>
	<button class="r-icon" onclick={() => onstep(1)} aria-label={t('reader.speech.next')} title={t('reader.speech.next')}>
		<ChevronRight size={20} />
	</button>

	<span class="sep" aria-hidden="true"></span>

	<button
		class="r-icon small"
		onclick={() => rate(-1)}
		disabled={prefs.rate <= RATE.min}
		aria-label={t('reader.speech.slower')}
		title={t('reader.speech.slower')}
	>
		<Minus size={16} />
	</button>
	<span class="rate" aria-live="polite" aria-label={t('reader.speech.rate')}>{prefs.rate.toFixed(1)}×</span>
	<button
		class="r-icon small"
		onclick={() => rate(1)}
		disabled={prefs.rate >= RATE.max}
		aria-label={t('reader.speech.faster')}
		title={t('reader.speech.faster')}
	>
		<Plus size={16} />
	</button>

	{#if voices.length > 1}
		<select value={chosen} onchange={(e) => pick(e.currentTarget.value)} aria-label={t('reader.speech.voice')} title={t('reader.speech.voice')}>
			{#each voices as voice (voice.voiceURI)}
				<option value={voice.voiceURI}>{voice.name}</option>
			{/each}
		</select>
	{/if}

	<span class="sep" aria-hidden="true"></span>

	<button class="r-icon" onclick={onstop} aria-label={t('reader.speech.stop')} title={t('reader.speech.stop')}>
		<X size={19} />
	</button>
</div>

<style>
	.speech {
		position: absolute;
		z-index: 3;
		left: 50%;
		bottom: calc(1.4rem + env(safe-area-inset-bottom));
		transform: translateX(-50%);
		display: flex;
		align-items: center;
		gap: 0.1rem;
		max-width: calc(100% - 1rem);
		padding: 0.15rem 0.35rem;
		border-radius: 99px;
		background: var(--r-surface);
		color: var(--r-fg);
		border: 1px solid var(--r-border);
		box-shadow: 0 2px 14px rgba(0, 0, 0, 0.16);
		transition: bottom 0.2s ease;
	}
	/* Clear of the expanded progress bar. */
	.speech.raised {
		bottom: calc(5.4rem + env(safe-area-inset-bottom));
	}
	.main {
		color: var(--r-link);
	}
	.small {
		min-width: 2rem;
	}
	.sep {
		width: 1px;
		height: 1.4rem;
		margin: 0 0.25rem;
		background: var(--r-border);
	}
	.rate {
		min-width: 2.6rem;
		text-align: center;
		font-size: 0.85rem;
		font-variant-numeric: tabular-nums;
		color: var(--r-muted);
	}
	select {
		max-width: 9rem;
		margin-left: 0.2rem;
		padding: 0.25rem 0.4rem;
		border-radius: 6px;
		border: 1px solid var(--r-border);
		background: var(--r-surface);
		color: var(--r-fg);
		font: inherit;
		font-size: 0.82rem;
	}
	/* A phone's width: everything on one row, the way out included. */
	@media (max-width: 30rem) {
		.speech :global(.r-icon) {
			min-width: 2.35rem;
			width: 2.35rem;
		}
		select {
			max-width: 4.6rem;
		}
		.sep {
			display: none;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.speech {
			transition: none;
		}
	}
</style>
