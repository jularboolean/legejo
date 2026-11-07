<script lang="ts">
	// The words page: per language, the reader's books with how much of each
	// they understand, the words worth learning next, and the words they know,
	// each with the books it occurs in.
	import { Check, GraduationCap } from '@lucide/svelte';
	import { getLocale, t } from '#lib/i18n';
	import { css, languageInSentence, loadLanguages, loadOverview, percent, setKnown, type Overview, type WordLanguage, type WordRow } from '#lib/library/vocab';

	const locale = $derived(getLocale());
	let languages = $state<WordLanguage[] | null>(null);
	let language = $state('');
	let overview = $state<Overview | null>(null);
	let filter = $state('');
	let showAll = $state(false);

	$effect(() => {
		loadLanguages().then((l) => {
			languages = l;
			if (!language && l.length) language = l[0].language;
		});
	});
	$effect(() => {
		const current = language;
		if (!current) return;
		overview = null;
		showAll = false;
		loadOverview(current).then((o) => {
			if (language === current) overview = o;
		});
	});

	const title = (code: string) => {
		const name = languageInSentence(code, locale);
		return name.charAt(0).toUpperCase() + name.slice(1);
	};
	const inBooks = (w: WordRow) => (w.books.length === 1 ? t('words.page.inBook') : t('words.page.inBooks', { count: w.books.length }));
	const bookTitles = (w: WordRow) =>
		w.books.map((id) => overview?.books.find((b) => b.id === id)?.title).filter(Boolean).join(' · ');

	const known = $derived.by(() => {
		const q = filter.trim().toLowerCase();
		const list = overview?.known ?? [];
		return q ? list.filter((w) => w.word.toLowerCase().includes(q) || (w.english ?? '').toLowerCase().includes(q)) : list;
	});
	const shownKnown = $derived(showAll || filter ? known : known.slice(0, 60));

	// Ticking moves a word between the lists; the books' shares follow on the next load.
	async function learn(w: WordRow, isKnown: boolean) {
		if (!overview) return;
		if (!(await setKnown(overview.language, w.stem, isKnown))) return;
		if (isKnown) {
			overview.next = overview.next.filter((x) => x.stem !== w.stem);
			overview.known = [w, ...overview.known];
		} else {
			overview.known = overview.known.filter((x) => x.stem !== w.stem);
		}
		const fresh = await loadOverview(overview.language);
		if (fresh && fresh.language === language) overview = fresh;
	}
</script>

<svelte:head><title>{t('sidebar.words')} – Legejo</title></svelte:head>

<h1>{t('sidebar.words')}</h1>
<p class="intro">{t('words.page.intro')}</p>

{#if languages && languages.length === 0}
	<p class="empty"><GraduationCap size={18} /> {t('words.page.none')}</p>
{:else if languages}
	{#if languages.length > 1}
		<div class="tabs" role="tablist">
			{#each languages as l (l.language)}
				<button role="tab" aria-selected={language === l.language} class:on={language === l.language} onclick={() => (language = l.language)}>
					{title(l.language)}
				</button>
			{/each}
		</div>
	{/if}

	{#if overview}
		{#if overview.books.length}
			<section>
				<h2>{t('words.page.books')}</h2>
				<p class="note">{t('words.page.booksNote')}</p>
				<ul class="books">
					{#each overview.books as b (b.id)}
						<li>
							<a href={`/books/${b.id}/words`}>
								<span class="title">{b.title}</span>
								{#if b.author}<span class="author">{b.author}</span>{/if}
							</a>
							<span class="bar" aria-hidden="true"><span style:width={css(b.share)}></span></span>
							<span class="pct">{percent(b.share)} %</span>
						</li>
					{/each}
				</ul>
			</section>
		{/if}

		{#if overview.next.length}
			<section>
				<h2>{t('words.page.next')}</h2>
				<p class="note">{t('words.page.nextNote')}</p>
				<ol class="words">
					{#each overview.next as w (w.stem)}
						<li>
							<button class="tick" aria-label={t('words.know', { word: w.word })} onclick={() => learn(w, true)}><Check size={15} /></button>
							<span class="word">
								<span class="head">{w.word}</span>
								{#if w.english}<span class="gloss">{w.english}</span>{/if}
							</span>
							<span class="where" title={bookTitles(w)}>{inBooks(w)}</span>
						</li>
					{/each}
				</ol>
			</section>
		{/if}

		<section>
			<h2>{t('words.page.known', { count: overview.known.length })}</h2>
			{#if overview.known.length > 8}
				<input class="filter" type="search" placeholder={t('words.page.filter')} bind:value={filter} />
			{/if}
			<ol class="words">
				{#each shownKnown as w (w.stem)}
					<li class="known">
						<button class="tick on" aria-label={t('words.page.forget', { word: w.word })} onclick={() => learn(w, false)}><Check size={15} /></button>
						<span class="word">
							<span class="head">{w.word}</span>
							{#if w.english}<span class="gloss">{w.english}</span>{/if}
						</span>
						{#if w.books.length}<span class="where" title={bookTitles(w)}>{inBooks(w)}</span>{/if}
					</li>
				{/each}
			</ol>
			{#if known.length > shownKnown.length}
				<button class="more" onclick={() => (showAll = true)}>{t('words.page.more')} ({known.length})</button>
			{/if}
		</section>

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
	.filter {
		margin: 0.2rem 0 0.6rem;
		width: min(100%, 20rem);
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
