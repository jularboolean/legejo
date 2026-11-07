<script lang="ts">
	// A book's study list: its words in order of frequency, ESPDIC's English
	// with each, the reader's known words ticked (shared by every book in the
	// language), and the list as a text file or an Anki import.
	import { ArrowLeft, BookOpen, Check, Download } from '@lucide/svelte';
	import { page } from '$app/state';
	import { getLocale, t } from '#lib/i18n';
	import { css, fmt, knownCover, loadVocab, percent, setKnown, WORD_CLASSES, type StudyRow, type Vocab, type WordClass } from '#lib/library/vocab';
	import type { MessageKey } from '#lib/i18n';

	const id = $derived(Number(page.params.id));
	let title = $state('');
	let vocab = $state<Vocab | null | undefined>(undefined);
	let known = $state(new Set<string>());
	let goal = $state<'80' | '90' | '95'>('80');
	let wordClass = $state<WordClass | 'all'>('all');

	// Known words hidden unless the reader has chosen to see them (remembered).
	const HIDE_KEY = 'legejo.words.hideKnown';
	let hideKnown = $state(true);
	try {
		hideKnown = localStorage.getItem(HIDE_KEY) !== 'false';
	} catch {
		// No storage: hidden, as by default.
	}
	$effect(() => {
		try {
			localStorage.setItem(HIDE_KEY, String(hideKnown));
		} catch {
			// Nowhere to remember it.
		}
	});

	$effect(() => {
		const current = id;
		vocab = undefined;
		loadVocab(current).then((v) => {
			if (id !== current) return;
			vocab = v;
			known = new Set((v?.study ?? []).filter((w) => w.known).map((w) => w.stem));
		});
		fetch(`/api/books/${current}`)
			.then((r) => (r.ok ? r.json() : null))
			.then((b) => {
				if (id === current) title = b?.title ?? '';
			})
			.catch(() => {});
	});

	const locale = $derived(getLocale());
	const n = (x: number) => fmt(x, locale);
	const goalCount = $derived(vocab ? (vocab.coverage[goal] ?? 0) : 0);
	const ranked = $derived(vocab?.study ?? []);
	const goalWords = $derived(ranked.filter((w) => !w.name).slice(0, goalCount));
	const words = $derived(wordClass === 'all' ? goalWords : goalWords.filter((w) => w.class === wordClass));
	const hasClasses = $derived(ranked.some((w) => w.class));
	const names = $derived(ranked.filter((w) => w.name).slice(0, 12));
	const share = $derived(vocab ? knownCover(vocab, known) : 0);
	const knownInList = $derived(words.filter((w) => known.has(w.stem)).length);
	const shown = $derived(hideKnown ? words.filter((w) => !known.has(w.stem)) : words);
	const rankOf = $derived(new Map(ranked.map((w, i) => [w.stem, i + 1])));

	async function toggle(w: StudyRow) {
		if (!vocab) return;
		const want = !known.has(w.stem);
		const next = new Set(known);
		if (want) next.add(w.stem);
		else next.delete(w.stem);
		known = next;
		if (!(await setKnown(vocab.language, w.stem, want))) {
			// Not taken: back as it was.
			const back = new Set(known);
			if (want) back.delete(w.stem);
			else back.add(w.stem);
			known = back;
		}
	}

	const gloss = (w: StudyRow) => (w.english ? (w.found_as ? `≈ ${w.found_as}: ${w.english}` : w.english) : '');
	const otherForms = (w: StudyRow) => w.forms.filter((f) => f !== w.word).slice(0, 3);

	// The reader's own dictionary lookup, for a translation in their language.
	type Entry = { headword: string; definitions: string[]; translations: { language: string; words: string[] }[]; link: string };
	let open = $state<string | null>(null);
	let meanings = $state<Record<string, Entry[] | 'loading' | 'none' | 'error'>>({});
	let sources = $state<Record<string, string>>({});
	async function explain(w: StudyRow) {
		open = open === w.stem ? null : w.stem;
		if (open === null || (meanings[w.stem] && meanings[w.stem] !== 'error')) return;
		meanings[w.stem] = 'loading';
		try {
			const res = await fetch(`/api/dictionary?word=${encodeURIComponent(w.word)}&language=${encodeURIComponent(vocab?.language ?? '')}`);
			if (!res.ok) {
				meanings[w.stem] = res.status === 404 ? 'none' : 'error';
				return;
			}
			const body = await res.json();
			sources[w.stem] = body.source;
			meanings[w.stem] = body?.entries?.length ? body.entries : 'none';
		} catch {
			meanings[w.stem] = 'error';
		}
	}
	// The reader's language first, then English, then whatever the dictionary has.
	const translation = (e: Entry) => {
		const mine = locale.split('-')[0];
		const tr = [mine, 'en'].map((l) => e.translations.find((x) => x.language === l)).find(Boolean) ?? e.translations[0];
		return tr ? (tr.language === mine ? tr.words.join(', ') : `${tr.words.slice(0, 4).join(', ')} (${tr.language})`) : '';
	};
	const sourceName = (s: string | undefined) => (s === 'revo' ? 'Reta Vortaro' : s === 'espdic' ? 'ESPDIC' : 'Wiktionary');

	// Export what the list shows: the goal's words, without the known ones when hidden.
	function download(name: string, text: string) {
		const url = URL.createObjectURL(new Blob([text], { type: 'text/plain;charset=utf-8' }));
		const a = Object.assign(document.createElement('a'), { href: url, download: name });
		a.click();
		setTimeout(() => URL.revokeObjectURL(url), 1000);
	}
	const fileBase = () => `${(title || t('words.fileName')).replace(/[\\/:*?"<>|]+/g, '').trim()} – ${t('words.fileName')} ${goal}`;
	function exportText() {
		const label = (w: StudyRow) => w.word + (otherForms(w).length ? ` (${otherForms(w).join(', ')})` : '');
		const width = Math.max(0, ...shown.map((w) => label(w).length));
		const lines = shown.map((w) => `${label(w).padEnd(width)}   ${gloss(w)}`.trimEnd());
		const head = [t('words.fileTitle', { title, count: shown.length, percent: goal })];
		if (vocab?.study_source) head.push(t('words.attribution', { source: vocab.study_source }));
		download(`${fileBase()}.txt`, [...head, '', ...lines, ''].join('\n'));
	}
	function exportAnki() {
		// Anki's text import: tab-separated, with file headers Anki reads itself.
		const esc = (x: string) => x.replace(/[\t\n]/g, ' ').replace(/&/g, '&amp;').replace(/</g, '&lt;');
		const deck = `Legejo::${(title || 'Legejo').replace(/::/g, ':')}`;
		const rows = shown.map((w) => {
			const forms = otherForms(w).length ? `<br><small>${esc(otherForms(w).join(' · '))}</small>` : '';
			return [esc(w.word), esc(gloss(w)) + forms, `legejo ${vocab?.language ?? ''}`.trim()].join('\t');
		});
		if (vocab?.study_source) {
			rows.push([esc(t('words.ankiAbout')), esc(t('words.ankiAboutText', { source: vocab.study_source })), 'legejo'].join('\t'));
		}
		const head = ['#separator:tab', '#html:true', '#notetype:Basic', `#deck:${deck}`, '#tags column:3'];
		download(`${fileBase()} (Anki).txt`, [...head, ...rows, ''].join('\n'));
	}
</script>

<svelte:head><title>{t('vocab.study')} · {title}</title></svelte:head>

<a class="back" href={`/books/${id}`}><ArrowLeft size={15} /> {title}</a>

{#if vocab?.study}
	<header>
		<h1>{t('vocab.study')}</h1>
		<p class="intro">{t('words.intro')}</p>
	</header>

	<div class="meter" role="img" aria-label={t('words.meter', { percent: percent(share) })}>
		<div class="track">
			<span class="fill" style:width={css(share)}></span>
			<span class="goal" style:left={css(Number(goal) / 100)}></span>
		</div>
		<div class="legend">
			<span>{t('words.meter', { percent: percent(share) })}</span>
			<span class="muted">{t('words.goal', { percent: goal })}</span>
		</div>
	</div>

	<div class="controls">
		<div class="seg" role="radiogroup" aria-label={t('words.goalLabel')}>
			{#each ['80', '90', '95'] as const as g (g)}
				<button role="radio" aria-checked={goal === g} class:on={goal === g} onclick={() => (goal = g)}>
					{g} %<small>{t('vocab.words', { count: n(vocab.coverage[g] ?? 0) })}</small>
				</button>
			{/each}
		</div>
		<label class="hide"><input type="checkbox" bind:checked={hideKnown} /> {t('words.hideKnown')}</label>
	</div>
	{#if hasClasses}
		<div class="classes" role="radiogroup" aria-label={t('words.class.all')}>
			{#each ['all', ...WORD_CLASSES] as const as c (c)}
				<button role="radio" aria-checked={wordClass === c} class:on={wordClass === c} onclick={() => (wordClass = c)}>
					{t(`words.class.${c}` as MessageKey)}
				</button>
			{/each}
		</div>
	{/if}
	<div class="status">
		<p class="muted small">{t('words.ticked', { known: knownInList, count: words.length })}</p>
		<span class="export">
			<Download size={14} />
			{t('words.export', { count: shown.length })}
			<button onclick={exportText}>{t('words.exportText')}</button> ·
			<button onclick={exportAnki}>{t('words.exportAnki')}</button>
		</span>
	</div>

	<ol class="list">
		{#each shown as w (w.stem)}
			<li class:known={known.has(w.stem)}>
				<button class="tick" aria-pressed={known.has(w.stem)} aria-label={t('words.know', { word: w.word })} onclick={() => toggle(w)}>
					<Check size={15} />
				</button>
				<button class="word" aria-expanded={open === w.stem} onclick={() => explain(w)}>
					<span class="line">
						<span class="head">{w.word}</span>
						<span class="forms">{otherForms(w).join(' · ')}</span>
						<span class="count">{n(w.count)}×</span>
					</span>
					{#if gloss(w)}<span class="gloss">{gloss(w)}</span>{/if}
				</button>
				<span class="rank">{rankOf.get(w.stem)}</span>
				{#if open === w.stem}
					<div class="meaning">
						{#if meanings[w.stem] === 'loading'}
							<span class="muted">{t('words.lookingUp')}</span>
						{:else if meanings[w.stem] === 'none'}
							<span class="muted">{t('words.noHit')}</span>
						{:else if meanings[w.stem] === 'error'}
							<span class="muted">{t('words.lookupFailed')}</span>
							<button class="retry" onclick={() => { open = null; explain(w); }}>{t('words.retry')}</button>
						{:else if Array.isArray(meanings[w.stem])}
							{#each (meanings[w.stem] as Entry[]).slice(0, 1) as e (e.headword)}
								{#if translation(e)}<p><b>{translation(e)}</b></p>{/if}
								{#if e.definitions[0]}<p class="muted">{e.definitions[0]}</p>{/if}
								{#if !translation(e) && !e.definitions[0]}<p class="muted">{t('words.noTranslation')}</p>{/if}
								{#if e.link}
									<p class="small">
										<a href={e.link} target="_blank" rel="noreferrer">{t('words.openArticle')}</a>
										<span class="muted">· {sourceName(sources[w.stem])}</span>
									</p>
								{/if}
							{/each}
						{/if}
					</div>
				{/if}
			</li>
		{/each}
	</ol>

	{#if names.length}
		<section class="names">
			<h2>{t('words.names')}</h2>
			<p class="muted small">{t('words.namesNote')}</p>
			<p>{names.map((x) => x.forms[0].charAt(0).toUpperCase() + x.forms[0].slice(1)).join(', ')}</p>
		</section>
	{/if}

	{#if vocab.study_source}<p class="muted small attribution">{t('words.attribution', { source: vocab.study_source })}</p>{/if}
	<a class="read" href={`/books/${id}/read`}><BookOpen size={16} /> {t('words.startReading')}</a>
{:else if vocab === null || (vocab && !vocab.study)}
	<p class="muted">{t('words.none')}</p>
{/if}

<style>
	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		color: var(--muted);
		margin-bottom: 1rem;
	}
	h1 {
		margin: 0 0 0.3rem;
	}
	.intro {
		margin: 0 0 0.6rem;
		color: var(--muted);
		max-width: 36rem;
	}
	.meter {
		position: sticky;
		top: 0;
		z-index: 2;
		background: var(--bg);
		padding: 0.6rem 0 0.4rem;
	}
	.track {
		position: relative;
		height: 10px;
		border-radius: 5px;
		background: var(--border);
	}
	.fill {
		position: absolute;
		inset: 0 auto 0 0;
		border-radius: 5px;
		background: var(--accent);
		transition: width 0.25s;
	}
	.goal {
		position: absolute;
		top: -4px;
		bottom: -4px;
		width: 2px;
		background: var(--fg);
	}
	.legend {
		display: flex;
		justify-content: space-between;
		gap: 1rem;
		margin-top: 0.35rem;
		font-size: 0.9rem;
	}
	.controls {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 0.6rem;
		margin-top: 0.8rem;
	}
	.seg {
		display: inline-flex;
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
	}
	.seg button {
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 0.3rem 0.8rem;
		border: 0;
		background: none;
		color: var(--fg);
		font: inherit;
		cursor: pointer;
	}
	.seg button + button {
		border-left: 1px solid var(--border);
	}
	.seg button.on {
		background: var(--accent);
		color: var(--bg);
	}
	.seg small {
		font-size: 0.72rem;
		opacity: 0.8;
	}
	.classes {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
		margin-top: 0.6rem;
	}
	.classes button {
		padding: 0.2rem 0.7rem;
		border-radius: 99px;
		border: 1px solid var(--border);
		background: none;
		color: var(--fg);
		font: inherit;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.classes button.on {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--bg);
	}
	.hide {
		font-size: 0.88rem;
		color: var(--muted);
	}
	.muted {
		color: var(--muted);
	}
	.small {
		font-size: 0.82rem;
		margin: 0.5rem 0 0;
	}
	.status {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: baseline;
		gap: 0.4rem 1rem;
	}
	.export {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.export button,
	.retry {
		border: 0;
		background: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		text-decoration: underline;
		cursor: pointer;
	}
	.retry {
		margin-left: 0.5rem;
	}
	.list {
		list-style: none;
		margin: 0.6rem 0 0;
		padding: 0;
		border-top: 1px solid var(--border);
	}
	.list li {
		display: grid;
		grid-template-columns: auto 1fr auto;
		align-items: center;
		gap: 0.6rem;
		padding: 0.15rem 0;
		border-bottom: 1px solid var(--border);
	}
	.tick {
		width: 30px;
		height: 30px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		border: 1.5px solid var(--border);
		background: none;
		color: transparent;
		cursor: pointer;
	}
	.known .tick {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--bg);
	}
	.word {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.1rem;
		min-width: 0;
		padding: 0.55rem 0;
		border: 0;
		background: none;
		color: var(--fg);
		font: inherit;
		text-align: left;
		cursor: pointer;
	}
	.line {
		width: 100%;
		display: flex;
		align-items: baseline;
		gap: 0.6rem;
		min-width: 0;
	}
	.head {
		font-size: 1.05rem;
		font-weight: 600;
	}
	.known .head {
		color: var(--muted);
		font-weight: 400;
	}
	.forms {
		flex: 1;
		min-width: 0;
		font-size: 0.82rem;
		color: var(--muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.gloss {
		width: 100%;
		text-align: left;
		font-size: 0.85rem;
		color: var(--muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.count,
	.rank {
		font-size: 0.78rem;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.rank {
		min-width: 2.2rem;
		text-align: right;
	}
	.meaning {
		grid-column: 2 / -1;
		padding: 0 0 0.6rem;
		font-size: 0.9rem;
	}
	.meaning p {
		margin: 0 0 0.2rem;
	}
	.names {
		margin-top: 1.5rem;
	}
	.names h2 {
		font-size: 1rem;
		margin: 0;
	}
	.attribution {
		margin-top: 1rem;
	}
	.read {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		margin: 1.5rem 0 2rem;
		padding: 0.55rem 1.1rem;
		border-radius: 8px;
		background: var(--accent);
		color: var(--bg);
		text-decoration: none;
	}
</style>
