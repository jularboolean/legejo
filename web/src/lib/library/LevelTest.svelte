<script lang="ts">
	// A short level test on the words page: a few words from each frequency
	// band of the reader's books, one at a time, answered "know" or "don't".
	// From the share known in each band the server ticks that many of the
	// band's commonest words, so a reader who already knows a lot does not
	// have to tick hundreds of words one by one.
	import { getLocale, t } from '#lib/i18n';
	import { fmt, loadLevelTest, sendLevelTest } from './vocab';

	let { language, onDone }: { language: string; onDone: () => void } = $props();

	type State = 'idle' | 'loading' | 'asking' | 'sending' | 'done' | 'failed';
	let step = $state<State>('idle');
	let queue = $state<{ stem: string; word: string }[]>([]);
	let at = $state(0);
	let answers = $state<{ stem: string; known: boolean }[]>([]);
	let result = $state<{ estimate: number; ticked: number } | null>(null);
	const locale = $derived(getLocale());

	async function start() {
		step = 'loading';
		const test = await loadLevelTest(language);
		if (!test || test.bands.length === 0) {
			step = 'failed';
			return;
		}
		// Commonest band first, so the test starts with words most readers know.
		queue = test.bands.flatMap((b) => b.words);
		answers = [];
		at = 0;
		step = 'asking';
	}

	async function answer(known: boolean) {
		answers.push({ stem: queue[at].stem, known });
		if (at + 1 < queue.length) {
			at += 1;
			return;
		}
		await finish();
	}

	async function finish() {
		step = 'sending';
		result = await sendLevelTest(language, answers);
		step = result ? 'done' : 'failed';
		if (result) onDone();
	}

	function back() {
		if (at === 0) return;
		at -= 1;
		answers.pop();
	}

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

<svelte:window onkeydown={onkey} />

<section class="level">
	<h2>{t('words.level.test.title')}</h2>
	{#if step === 'idle' || step === 'loading'}
		<p class="note">{t('words.level.test.intro')}</p>
		<button class="primary" onclick={start} disabled={step === 'loading'}>{t('words.level.test.start')}</button>
	{:else if step === 'asking'}
		<p class="note">{t('words.level.test.how')}</p>
		<div class="card" aria-live="polite">
			<span class="count">{at + 1} / {queue.length}</span>
			<span class="word" lang={language}>{queue[at].word}</span>
			<span class="progress" aria-hidden="true"><span style:width={`${(at / queue.length) * 100}%`}></span></span>
		</div>
		<div class="answers">
			<button onclick={() => answer(false)}>{t('words.level.test.no')}</button>
			<button class="primary" onclick={() => answer(true)}>{t('words.level.test.yes')}</button>
		</div>
		<p class="keys">
			{#if at > 0}<button class="link" onclick={back}>{t('words.level.test.back')}</button> ·{/if}
			<button class="link" onclick={finish}>{t('words.level.test.stop')}</button>
		</p>
	{:else if step === 'sending'}
		<p class="note">{t('words.level.test.sending')}</p>
	{:else if step === 'done' && result}
		<p>{t('words.level.test.result', { estimate: fmt(result.estimate, locale), ticked: fmt(result.ticked, locale) })}</p>
		<p class="note">{t('words.level.test.after')}</p>
		<button class="link" onclick={() => (step = 'idle')}>{t('words.level.test.close')}</button>
	{:else}
		<p class="note">{t('words.level.test.failed')}</p>
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
	.primary {
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
