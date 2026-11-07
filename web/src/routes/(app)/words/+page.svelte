<script lang="ts">
	// The words page: per language, the reader's books with how much of each
	// they understand, the words worth learning next, and the words they know,
	// each with the books it occurs in.
	import { Check, Download, GraduationCap, Quote } from '@lucide/svelte';
	import { untrack } from 'svelte';
	import { getLocale, t } from '#lib/i18n';
	import WordsOption from '#lib/library/WordsOption.svelte';
	import LevelTest from '#lib/library/LevelTest.svelte';
	import {
		ankiDeck,
		appendUnique,
		asAnki,
		asText,
		css,
		download,
		fmt,
		languageInSentence,
		loadKnownWords,
		loadLanguages,
		loadBooks,
		loadExamples,
		loadOverview,
		loadSearch,
		percent,
		setKnown,
		WORD_CLASSES,
		type BookShare,
		type Example,
		type KnownSort,
		type Overview,
		type WordClass,
		type WordLanguage,
		type WordRow
	} from '#lib/library/vocab';
	import type { MessageKey } from '#lib/i18n';

	let { data } = $props();

	/** Known words shown at first, and how many more each "show more" brings. */
	const FIRST = 20;
	/** Below this many known words the level test comes first on the page. */
	const LEVEL_TEST_BELOW = 100;
	const PAGE = 100;
	const SORTS: KnownSort[] = ['recent', 'common', 'alpha'];
	const SORT_KEY = 'legejo.words.knownSort';

	const locale = $derived(getLocale());
	let languages = $state<WordLanguage[] | null>(null);
	let language = $state('');
	let overview = $state<Overview | null>(null);
	let levelFirst = $state(true);

	$effect(() => {
		if (!data.words) return;
		loadLanguages().then((l) => {
			languages = l;
			if (!language && l.length) language = l[0].language;
		});
	});
	$effect(() => {
		const current = language;
		if (!current) return;
		overview = null;
		loadOverview(current).then((o) => {
			if (language !== current) return;
			overview = o;
			// Where the level test stands is settled when the page opens, so
			// that it does not move away while it shows its result.
			levelFirst = (o?.known ?? 0) < LEVEL_TEST_BELOW;
		});
	});

	// The books: the first come with the overview, the rest a page at a time.
	const BOOK_PAGE = 50;
	let books = $state<BookShare[]>([]);
	let booksTotal = $state(0);
	let loadingBooks = $state(false);
	let sameLanguage = '';
	$effect(() => {
		if (!overview) return;
		const current = overview.language;
		const fresh = overview.books;
		booksTotal = overview.books_total;
		// After a tick the shares change: as many books as were shown, fresh;
		// a new language starts on the first ones.
		const shown = untrack(() => (books[0] && fresh[0] && books.length > fresh.length && sameLanguage === current ? books.length : 0));
		sameLanguage = current;
		if (shown) {
			loadBooks(current, 0, shown).then((page) => {
				if (page && language === current) books = page.books;
			});
		} else {
			books = fresh;
		}
	});
	async function moreBooks() {
		if (!overview) return;
		loadingBooks = true;
		const current = overview.language;
		const page = await loadBooks(current, books.length, BOOK_PAGE);
		loadingBooks = false;
		if (page && language === current) {
			books = appendUnique(books, page.books, (b) => b.id);
			booksTotal = page.total;
		}
	}
	async function refresh() {
		if (!overview) return;
		const current = overview.language;
		const [fresh] = await Promise.all([loadOverview(current), fetchKnown(0, Math.max(FIRST, knownWords.length))]);
		if (fresh && fresh.language === language) overview = fresh;
	}

	const title = (code: string) => {
		const name = languageInSentence(code, locale);
		return name.charAt(0).toUpperCase() + name.slice(1);
	};
	const inBooks = (w: WordRow) => (w.books === 1 ? t('words.page.inBook') : t('words.page.inBooks', { count: w.books }));
	// The titles of the books shown so far, for a word's tooltip.
	const titles = $derived(new Map(books.map((b) => [b.id, b.title])));
	const bookTitles = (w: WordRow) => w.book_ids.map((id) => titles.get(id)).filter(Boolean).join(' · ');

	// The words the reader knows: asked of the server a page at a time, in the
	// chosen order, class and filter.
	let sort = $state<KnownSort>(readSort());
	let wordClass = $state<WordClass | 'all'>('all');
	let filter = $state('');
	let knownWords = $state<WordRow[]>([]);
	let knownTotal = $state(0);
	let loadingKnown = $state(false);
	let asked = 0;

	function readSort(): KnownSort {
		try {
			const s = localStorage.getItem(SORT_KEY);
			if (s && (SORTS as string[]).includes(s)) return s as KnownSort;
		} catch {
			// No storage; the default order.
		}
		return 'recent';
	}
	function chooseSort(s: KnownSort) {
		sort = s;
		try {
			localStorage.setItem(SORT_KEY, s);
		} catch {
			// Kept for this visit only.
		}
	}

	async function fetchKnown(offset: number, limit: number) {
		const current = language;
		const ask = ++asked;
		loadingKnown = true;
		const page = await loadKnownWords(current, { q: filter.trim(), sort, class: wordClass, offset, limit });
		if (ask !== asked || language !== current) return;
		loadingKnown = false;
		if (!page) return;
		knownTotal = page.total;
		knownWords = offset ? appendUnique(knownWords, page.words, (w) => w.stem) : page.words;
	}
	$effect(() => {
		// Asked again when the language, order, class or filter changes.
		const current = language;
		void sort;
		void wordClass;
		const typed = filter.trim();
		if (!current) return;
		const timer = setTimeout(() => fetchKnown(0, FIRST), typed ? 250 : 0);
		return () => clearTimeout(timer);
	});
	$effect(() => {
		// A new language starts on all classes and no filter, with no words
		// of the last one left under its heading.
		void language;
		wordClass = 'all';
		filter = '';
		knownWords = [];
		knownTotal = 0;
	});
	const showMore = () => fetchKnown(knownWords.length, PAGE);
	const filtered = $derived(!!filter.trim() || wordClass !== 'all');

	// Example sentences, asked for only when the reader opens them: a
	// sentence from later in a book can give away what happens. Open per
	// list, as a word can stand in more than one.
	type Section = 'matches' | 'next' | 'known';
	let examples = $state<Record<string, Example[] | 'loading' | 'failed'>>({});
	async function toggleExamples(section: Section, w: WordRow) {
		const key = `${section}:${w.stem}`;
		if (examples[key]) {
			delete examples[key];
			return;
		}
		const current = language;
		examples[key] = 'loading';
		const found = await loadExamples(current, w.stem);
		if (language !== current || !examples[key]) return;
		examples[key] = found ?? 'failed';
	}
	$effect(() => {
		void language;
		examples = {};
	});

	// The search among all the words of the reader's books, asked as one types.
	let search = $state('');
	let matches = $state<WordRow[] | null>(null);
	$effect(() => {
		const q = search.trim();
		const current = language;
		if (!q || !current) {
			matches = null;
			return;
		}
		const timer = setTimeout(() => {
			loadSearch(current, q).then((found) => {
				if (search.trim() === q && language === current) matches = found ?? [];
			});
		}, 250);
		return () => clearTimeout(timer);
	});

	async function exportKnown(kind: 'text' | 'anki') {
		if (!overview) return;
		const all = await loadKnownWords(overview.language, { sort: 'alpha' });
		if (!all) return;
		const name = languageInSentence(overview.language, locale);
		const title = t('words.page.fileTitle', { language: name, count: all.total });
		const rows = all.words.map((w) => ({ word: w.word, english: w.english }));
		if (kind === 'text') {
			const head = [title];
			if (overview.source) head.push(t('words.attribution', { source: overview.source }));
			download(`${title}.txt`, asText(head, rows));
		} else {
			const about: [string, string] | undefined = overview.source
				? [t('words.ankiAbout'), t('words.ankiAboutText', { source: overview.source })]
				: undefined;
			download(`${title} (Anki).txt`, asAnki(ankiDeck(name), `legejo ${overview.language}`, rows, about));
		}
	}

	// Ticking a word moves it between the lists; the counts and the books'
	// shares follow from the server. A tick the server did not take is said
	// so for a moment under the heading.
	let saveError = $state('');
	let errorTimer: ReturnType<typeof setTimeout> | undefined;
	function sayFailed() {
		saveError = t('edit.saveFailed');
		clearTimeout(errorTimer);
		errorTimer = setTimeout(() => (saveError = ''), 4000);
	}
	$effect(() => () => clearTimeout(errorTimer));
	async function learn(w: WordRow, known: boolean) {
		if (!overview) return;
		const current = overview.language;
		const taken = await setKnown(current, w.stem, known);
		if (language !== current) return;
		if (!taken) {
			sayFailed();
			return;
		}
		if (matches) matches = matches.map((m) => (m.stem === w.stem ? { ...m, known } : m));
		if (known) overview.next = overview.next.filter((x) => x.stem !== w.stem);
		else knownWords = knownWords.filter((x) => x.stem !== w.stem);
		await refresh();
	}
</script>

{#snippet wordRow(section: Section, w: WordRow, on: boolean)}
	{@const open = examples[`${section}:${w.stem}`]}
	<li class:known={on}>
		<button class="tick" class:on aria-pressed={on} aria-label={t('words.know', { word: w.word })} onclick={() => learn(w, !on)}>
			<Check size={15} />
		</button>
		<span class="word">
			<span class="head">{w.word}</span>
			{#if w.english}<span class="gloss">{w.english}</span>{/if}
		</span>
		<span class="side">
			{#if w.books}
				<span class="where" title={bookTitles(w)}>{inBooks(w)}</span>
				<button class="ex-toggle" aria-expanded={!!open} onclick={() => toggleExamples(section, w)}>
					<Quote size={12} />
					{open ? t('words.examples.hide') : t('words.examples.show')}
				</button>
			{/if}
		</span>
		{#if open}
			<div class="examples" aria-label={t('words.examples.label', { word: w.word })} aria-live="polite">
				{#if open === 'loading'}
					<p class="note">{t('words.examples.loading')}</p>
				{:else if open === 'failed'}
					<p class="note">{t('words.examples.failed')}</p>
				{:else if open.length === 0}
					<p class="note">{t('words.examples.none')}</p>
				{:else}
					{#each open as ex, i (i)}
						<blockquote>
							<p>{#each ex.pieces as piece, j (j)}{#if piece.hit}<mark>{piece.text}</mark>{:else}{piece.text}{/if}{/each}</p>
							<cite><a href={`/books/${ex.book}`}>{ex.title}</a></cite>
						</blockquote>
					{/each}
				{/if}
			</div>
		{/if}
	</li>
{/snippet}

<svelte:head><title>{t('sidebar.words')} – Legejo</title></svelte:head>

<h1>{t('sidebar.words')}</h1>
<p class="intro">{t('words.page.intro')}</p>
{#if saveError}<p class="error" role="alert">{saveError}</p>{/if}

{#if data.offline}
	<p class="empty"><GraduationCap size={18} /> {t('words.offline')}</p>
{:else if !data.words}
	<WordsOption />
{:else if languages && languages.length === 0}
	<p class="empty"><GraduationCap size={18} /> {t('words.page.none')}</p>
{:else if languages}
	{#if languages.length > 1}
		<div class="tabs">
			{#each languages as l (l.language)}
				<button aria-pressed={language === l.language} class:on={language === l.language} onclick={() => (language = l.language)}>
					{title(l.language)}
				</button>
			{/each}
		</div>
	{/if}

	{#if overview}
		<input class="search" type="search" placeholder={t('words.page.search')} bind:value={search} />
		{#if matches}
			<section>
				<h2>{t('words.page.matches')}</h2>
				{#if matches.length === 0}
					<p class="note">{t('words.page.noMatch')}</p>
				{:else}
					<ol class="words">
						{#each matches as w (w.stem)}
							{@render wordRow('matches', w, w.known)}
						{/each}
					</ol>
				{/if}
			</section>
		{/if}
		{#if levelFirst}
			<LevelTest language={overview.language} onDone={refresh} />
		{/if}

		{#if books.length}
			<section>
				<h2>{t('words.page.books')}</h2>
				<p class="note">{t('words.page.booksNote')}</p>
				<ul class="books">
					{#each books as b (b.id)}
						<li>
							<a href={`/books/${b.id}/words`}>
								<span class="title">{b.title}</span>
								<span class="author">
									{#if b.reading}<span class="reading">{t('words.page.reading')}</span>{/if}
									{b.author ?? ''}
								</span>
							</a>
							<span class="bar" aria-hidden="true"><span style:width={css(b.share)}></span></span>
							<span class="pct">{percent(b.share)} %</span>
						</li>
					{/each}
				</ul>
				{#if booksTotal > books.length}
					<button class="more" onclick={moreBooks} disabled={loadingBooks}>
						{t('words.page.moreBooks', { count: fmt(booksTotal - books.length, locale) })}
					</button>
				{/if}
			</section>
		{/if}

		{#if overview.next.length}
			<section>
				<h2>{t('words.page.next')}</h2>
				<p class="note">{t('words.page.nextNote')}</p>
				<ol class="words">
					{#each overview.next as w (w.stem)}
						{@render wordRow('next', w, false)}
					{/each}
				</ol>
			</section>
		{/if}

		<section>
			<h2>{t('words.page.known', { count: fmt(overview.known, locale) })}</h2>
			{#if overview.known}
				{#if overview.common.of}
					<p class="note">
						{t('words.page.common', {
							known: fmt(overview.common.known, locale),
							of: fmt(overview.common.of, locale),
							percent: percent(overview.common.known / overview.common.of)
						})}
					</p>
					<span class="bar common" aria-hidden="true"><span style:width={css(overview.common.known / overview.common.of)}></span></span>
				{/if}
				<p class="export">
					<Download size={14} />
					{t('words.export', { count: fmt(overview.known, locale) })}
					<button onclick={() => exportKnown('text')}>{t('words.exportText')}</button> ·
					<button onclick={() => exportKnown('anki')}>{t('words.exportAnki')}</button>
				</p>
				<div class="controls">
					<input class="filter" type="search" placeholder={t('words.page.filter')} bind:value={filter} />
					<label class="sort">
						{t('words.page.sort')}
						<select value={sort} onchange={(e) => chooseSort((e.currentTarget as HTMLSelectElement).value as KnownSort)}>
							{#each SORTS as s (s)}
								<option value={s}>{t(`words.page.sort.${s}` as MessageKey)}</option>
							{/each}
						</select>
					</label>
				</div>
				<div class="classes" role="group" aria-label={t('words.class.all')}>
					{#each ['all', ...WORD_CLASSES] as const as c (c)}
						<button aria-pressed={wordClass === c} class:on={wordClass === c} onclick={() => (wordClass = c)}>
							{t(`words.class.${c}` as MessageKey)}
						</button>
					{/each}
				</div>
				{#if filtered && knownTotal === 0 && !loadingKnown}
					<p class="note">{t('words.page.noKnown')}</p>
				{/if}
				<ol class="words" class:loading={loadingKnown}>
					{#each knownWords as w (w.stem)}
						{@render wordRow('known', w, true)}
					{/each}
				</ol>
				{#if knownTotal > knownWords.length}
					<button class="more" onclick={showMore} disabled={loadingKnown}>
						{t('words.page.more', { count: fmt(knownTotal - knownWords.length, locale) })}
					</button>
				{/if}
			{/if}
		</section>

		{#if !levelFirst}
			<LevelTest language={overview.language} onDone={refresh} />
		{/if}

		{#if overview.source}<p class="note attribution">{t('words.attribution', { source: overview.source })}</p>{/if}
	{/if}
{/if}

<style>
	h1 {
		margin: 0 0 0.3rem;
	}
	.intro {
		color: var(--muted);
		max-width: 38rem;
		margin: 0 0 1rem;
	}
	.empty {
		display: flex;
		gap: 0.5rem;
		align-items: flex-start;
		color: var(--muted);
		max-width: 34rem;
	}
	.error {
		margin: 0 0 0.8rem;
		font-size: 0.9rem;
		color: var(--danger, #c0392b);
	}
	.tabs {
		display: flex;
		gap: 0.4rem;
		flex-wrap: wrap;
		margin-bottom: 0.5rem;
	}
	.tabs button {
		padding: 0.3rem 0.9rem;
		border-radius: 99px;
		border: 1px solid var(--border);
		background: none;
		color: var(--fg);
		font: inherit;
		cursor: pointer;
	}
	.tabs button.on {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--bg);
	}
	section {
		margin-top: 1.4rem;
	}
	h2 {
		font-size: 1.05rem;
		margin: 0;
	}
	.note {
		margin: 0.1rem 0 0.6rem;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.books {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.books li {
		display: grid;
		grid-template-columns: 1fr 8rem 3rem;
		align-items: center;
		gap: 0.8rem;
	}
	@media (max-width: 40rem) {
		.books li {
			grid-template-columns: 1fr 4.5rem 2.6rem;
		}
	}
	.books a {
		display: flex;
		flex-direction: column;
		min-width: 0;
		color: var(--fg);
		text-decoration: none;
	}
	.title {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.author {
		font-size: 0.8rem;
		color: var(--muted);
	}
	.reading {
		display: inline-block;
		margin-right: 0.3rem;
		padding: 0 0.35rem;
		border-radius: 99px;
		background: var(--accent);
		color: var(--bg);
		font-size: 0.72rem;
	}
	.bar {
		height: 6px;
		border-radius: 3px;
		background: var(--border);
		overflow: hidden;
	}
	.bar span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.pct {
		font-size: 0.85rem;
		color: var(--muted);
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.words {
		list-style: none;
		padding: 0;
		margin: 0;
		border-top: 1px solid var(--border);
	}
	.words li {
		display: grid;
		grid-template-columns: auto 1fr auto;
		align-items: center;
		gap: 0.6rem;
		padding: 0.35rem 0;
		border-bottom: 1px solid var(--border);
	}
	.tick {
		position: relative;
		width: 28px;
		height: 28px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		border: 1.5px solid var(--border);
		background: none;
		color: transparent;
		cursor: pointer;
	}
	/* A finger's target of 40 px around the circle. */
	.tick::before {
		content: '';
		position: absolute;
		inset: -6px;
	}
	.tick:hover {
		color: var(--muted);
	}
	.tick.on {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--bg);
	}
	.word {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.head {
		font-weight: 600;
	}
	.known .head {
		font-weight: 400;
	}
	.gloss {
		font-size: 0.85rem;
		color: var(--muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.where {
		font-size: 0.78rem;
		color: var(--muted);
		white-space: nowrap;
	}
	.side {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 0.15rem;
	}
	.ex-toggle {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		border: 0;
		background: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		font-size: 0.78rem;
		cursor: pointer;
		white-space: nowrap;
	}
	.examples {
		grid-column: 2 / -1;
		margin: 0.1rem 0 0.3rem;
	}
	.examples .note {
		margin: 0;
	}
	blockquote {
		margin: 0 0 0.5rem;
		padding: 0.1rem 0 0.1rem 0.7rem;
		border-left: 2px solid var(--border);
	}
	blockquote p {
		margin: 0;
		font-size: 0.92rem;
		line-height: 1.45;
	}
	blockquote mark {
		background: none;
		color: inherit;
		font-weight: 600;
		text-decoration: underline;
		text-decoration-color: var(--accent);
		text-underline-offset: 2px;
	}
	cite {
		display: block;
		margin-top: 0.15rem;
		font-size: 0.78rem;
		font-style: normal;
		color: var(--muted);
	}
	cite a {
		color: inherit;
	}
	.search {
		width: min(100%, 26rem);
		margin: 0.4rem 0 0;
	}
	.export {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.3rem;
		margin: 0.1rem 0 0.6rem;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.export button {
		border: 0;
		background: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		text-decoration: underline;
		cursor: pointer;
	}
	.controls {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem 1rem;
		margin: 0.2rem 0 0;
	}
	.filter {
		width: min(100%, 20rem);
	}
	.sort {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.classes {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
		margin: 0.6rem 0;
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
	.bar.common {
		display: block;
		max-width: 26rem;
		margin: 0 0 0.7rem;
	}
	.words.loading {
		opacity: 0.6;
	}
	.more {
		margin-top: 0.6rem;
		border: 0;
		background: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		text-decoration: underline;
		cursor: pointer;
	}
	.attribution {
		margin-top: 1.5rem;
	}
</style>
