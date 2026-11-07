<script lang="ts">
	// A short level test on the words page: a few words from each frequency
	// band of the reader's books, one at a time, answered "know" or "don't".
	// From the share known in each band the server ticks that many of the
	// band's commonest words, so a reader who already knows a lot does not
	// have to tick hundreds of words one by one.
	import { tick } from 'svelte';
	import { Check, X } from '@lucide/svelte';
	import { getLocale, t } from '#lib/i18n';
	import { fmt, loadLevelTest, sendLevelTest } from './vocab';

	let { language, onDone }: { language: string; onDone: () => void } = $props();

	type State = 'idle' | 'loading' | 'asking' | 'sending' | 'done' | 'failed';
	let step = $state<State>('idle');
	let queue = $state<{ stem: string; word: string; english: string | null }[]>([]);
	/** The meaning shown after a "yes", for the reader to check before going on. */
	let checking = $state(false);
	let at = $state(0);
	let answers = $state<{ stem: string; known: boolean }[]>([]);
	let result = $state<{ estimate: number; ticked: number } | null>(null);
	const locale = $derived(getLocale());
	/** The card: it takes the focus while the test runs, so that the keys answer it. */
	let card = $state<HTMLElement | null>(null);

	async function focusCard() {
		await tick();
		card?.focus();
	}

	async function start() {
		step = 'loading';
		const test = await loadLevelTest(language);
		if (!test || test.bands.length === 0) {
			step = 'failed';
			focusCard();
			return;
		}
		// Commonest band first, so the test starts with words most readers know.
		queue = test.bands.flatMap((b) => b.words);
		answers = [];
		at = 0;
		checking = false;
		step = 'asking';
		focusCard();
	}

	// A "yes" shows the meaning first; the answer counts once the reader has
	// seen it and goes on, or takes it back.
	async function answer(known: boolean) {
		if (known && !checking && queue[at].english) {
			checking = true;
			return;
		}
		checking = false;
		answers.push({ stem: queue[at].stem, known });
		if (at + 1 < queue.length) {
			at += 1;
			return;
		}
		await finish();
	}

	async function finish() {
		// Stopped before the first answer: nothing to send.
		if (answers.length === 0) {
			step = 'idle';
			return;
		}
		step = 'sending';
		result = await sendLevelTest(language, answers);
		step = result ? 'done' : 'failed';
		if (result) onDone();
		focusCard();
	}

	function back() {
		if (checking) {
			checking = false;
			return;
		}
		if (at === 0) return;
		at -= 1;
		answers.pop();
	}

	// Keys answer while the card or something in it has the focus. Enter is
	// left to the buttons, which answer on it themselves.
	function onkey(e: KeyboardEvent) {
		if (step !== 'asking' || e.metaKey || e.ctrlKey || e.altKey) return;
		const target = e.target as HTMLElement | null;
		if (target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) return;
		if (e.key === 'ArrowRight' || e.key === 'j') answer(true);
		else if (e.key === 'ArrowLeft' || e.key === 'n') answer(false);
		else if (e.key === 'Backspace') back();
		else return;
		e.preventDefault();
	}
</script>

<!-- The keys are shortcuts for the buttons below, which do the same. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section class="level" role="group" aria-labelledby="level-test-title" tabindex="-1" bind:this={card} onkeydown={onkey}>
	<h2 id="level-test-title">{t('words.level.test.title')}</h2>
	{#if step === 'idle' || step === 'loading'}
		<p class="note">{t('words.level.test.intro')}</p>
		<button class="primary" onclick={start} disabled={step === 'loading'}>{t('words.level.test.start')}</button>
	{:else if step === 'asking'}
		<p class="note">{t('words.level.test.how')}</p>
		<div class="card" aria-live="polite">
			<span class="count">{at + 1} / {queue.length}</span>
			<span class="word" lang={language}>{queue[at].word}</span>
			<span class="english" class:shown={checking}>{checking ? queue[at].english : ''}</span>
			<span class="progress" aria-hidden="true"><span style:width={`${(at / queue.length) * 100}%`}></span></span>
		</div>
		{#if checking}
			<p class="check">{t('words.level.test.check')}</p>
			<div class="answers">
				<button class="wrong" onclick={() => answer(false)}><X size={18} /> {t('words.level.test.wrong')}</button>
				<button class="primary" onclick={() => answer(true)}>{t('words.level.test.next')}</button>
			</div>
		{:else}
			<div class="round">
				<button class="no" aria-label={t('words.level.test.no')} title={t('words.level.test.no')} onclick={() => answer(false)}>
					<X size={30} strokeWidth={2.5} />
				</button>
				<button class="yes" aria-label={t('words.level.test.yes')} title={t('words.level.test.yes')} onclick={() => answer(true)}>
					<Check size={30} strokeWidth={2.5} />
				</button>
			</div>
		{/if}
		<p class="keys">
			{#if at > 0}<button class="link" onclick={back}>{t('words.level.test.back')}</button> ·{/if}
			<button class="link" onclick={finish}>{t('words.level.test.stop')}</button>
		</p>
	{:else if step === 'sending'}
		<p class="note">{t('words.level.test.sending')}</p>
	{:else if step === 'done' && result}
		<div aria-live="polite">
			<p>{t('words.level.test.result', { estimate: fmt(result.estimate, locale), ticked: fmt(result.ticked, locale) })}</p>
			<p class="note">{t('words.level.test.after')}</p>
		</div>
		<button class="link" onclick={() => (step = 'idle')}>{t('words.level.test.close')}</button>
	{:else}
		<p class="note" aria-live="polite">{t('words.level.test.failed')}</p>
		<button class="link" onclick={() => (step = 'idle')}>{t('words.level.test.close')}</button>
	{/if}
</section>

<style>
	.level {
		margin-top: 1.4rem;
		max-width: 34rem;
		padding: 0.9rem 1rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
	}
	/* Focused by the test itself, not by tabbing: no ring then. */
	.level:focus:not(:focus-visible) {
		outline: none;
	}
	h2 {
		font-size: 1.05rem;
		margin: 0 0 0.3rem;
	}
	p {
		margin: 0 0 0.7rem;
	}
	.note {
		font-size: 0.88rem;
		color: var(--muted);
	}
	.card {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.4rem;
		padding: 1.2rem 0 0.9rem;
	}
	.count {
		font-size: 0.8rem;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.word {
		font-size: 1.8rem;
		font-weight: 600;
		overflow-wrap: anywhere;
		text-align: center;
	}
	.english {
		min-height: 1.4em;
		max-width: 26rem;
		text-align: center;
		color: var(--muted);
		opacity: 0;
		transition: opacity 0.15s;
	}
	.english.shown {
		opacity: 1;
	}
	.check {
		text-align: center;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.round {
		display: flex;
		justify-content: center;
		gap: 2.5rem;
		padding: 0.2rem 0 0.3rem;
	}
	.round button {
		width: 64px;
		height: 64px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		border: 2px solid var(--border);
		background: none;
		color: var(--fg);
		transition:
			transform 0.1s,
			border-color 0.1s;
	}
	.round button:active {
		transform: scale(0.94);
	}
	.round .no:hover {
		border-color: var(--danger, #c0392b);
		color: var(--danger, #c0392b);
	}
	.round .yes {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--bg);
	}
	.wrong {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.3rem;
	}
	.progress {
		width: min(100%, 14rem);
		height: 4px;
		border-radius: 2px;
		background: var(--border);
		overflow: hidden;
	}
	.progress span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.answers {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.6rem;
	}
	button {
		font: inherit;
		cursor: pointer;
	}
	.answers button,
	.primary {
		padding: 0.55rem 1rem;
		border-radius: 6px;
		border: 1px solid var(--border);
		background: none;
		color: var(--fg);
	}
	.primary,
	.answers .primary {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--bg);
	}
	.keys {
		margin: 0.7rem 0 0;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.link {
		border: 0;
		background: none;
		padding: 0;
		color: var(--accent);
		text-decoration: underline;
		font-size: 0.88rem;
	}
</style>
