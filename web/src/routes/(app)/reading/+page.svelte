<script lang="ts">
	import { goto } from '$app/navigation';
	import { getLocale, t } from '#lib/i18n';
	import { languageLabel } from '#lib/library';
	import ReadingRow from '#lib/library/ReadingRow.svelte';
	import { readingStats, type Month, type Tally } from '#lib/readingStats';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const s = $derived(readingStats(data.books));
	const ROW = 12;
	const percentRead = $derived(s.total ? Math.round((s.finished.length / s.total) * 100) : 0);

	const monthFmt = $derived(new Intl.DateTimeFormat(getLocale() === 'en' ? 'en-GB' : getLocale(), { month: 'short' }));
	const max = (xs: number[]) => Math.max(1, ...xs);

	function show(status: string) {
		goto(`/?status=${status}`);
	}
</script>

{#snippet bars(months: Month[], label: string)}
	{@const top = max(months.map((m) => m.count))}
	<figure class="chart" aria-label={label}>
		<div class="cols">
			{#each months as m (m.key)}
				<div class="col" title={`${monthFmt.format(m.date)} ${m.date.getFullYear()}: ${m.count}`}>
					<span class="val">{m.count || ''}</span>
					<span class="bar" style:height={`${(m.count / top) * 100}%`}></span>
					<span class="mon">{monthFmt.format(m.date).replace('.', '')}</span>
				</div>
			{/each}
		</div>
	</figure>
{/snippet}

{#snippet tally(items: Tally[], format?: (label: string) => string)}
	{@const top = max(items.map((i) => i.count))}
	<ul class="tally">
		{#each items as item (item.label)}
			<li>
				<span class="t-label">{format ? format(item.label) : item.label}</span>
				<span class="t-bar"><span style:width={`${(item.count / top) * 100}%`}></span></span>
				<span class="t-n">{item.count}</span>
			</li>
		{/each}
	</ul>
{/snippet}

<h1>{t('reading.heading')}</h1>

{#if s.total === 0}
	<p class="empty">{t('home.empty')}</p>
{:else}
	<div class="tiles">
		<div class="tile"><span class="big">{s.total}</span><span>{t('reading.tile.books')}</span></div>
		<button type="button" class="tile" onclick={() => show('finished')}>
			<span class="big">{s.finished.length}</span><span>{t('reading.tile.finished', { percent: percentRead })}</span>
		</button>
		<button type="button" class="tile" onclick={() => show('reading')}>
			<span class="big">{s.reading.length}</span><span>{t('reading.tile.reading')}</span>
		</button>
		<button type="button" class="tile" onclick={() => show('wanted')}>
			<span class="big">{s.wanted.length}</span><span>{t('reading.tile.wanted')}</span>
		</button>
		<button type="button" class="tile" onclick={() => show('unread')}>
			<span class="big">{s.unread}</span><span>{t('reading.tile.unread')}</span>
		</button>
	</div>

	{#if s.reading.length > 0}
		<ReadingRow title={t('home.continue')} books={s.reading.slice(0, ROW)} total={s.reading.length} onall={() => show('reading')} />
	{/if}
	{#if s.wanted.length > 0}
		<ReadingRow title={t('home.wantToRead')} books={s.wanted.slice(0, ROW)} total={s.wanted.length} onall={() => show('wanted')} />
	{:else}
		<p class="hint">{t('reading.wantedHint')}</p>
	{/if}
	{#if s.recentlyFinished.length > 0}
		<ReadingRow
			title={t('reading.recentlyFinished')}
			books={s.recentlyFinished.slice(0, ROW)}
			total={s.recentlyFinished.length}
			onall={() => show('finished')}
		/>
	{/if}

	<h2 class="section">{t('reading.stats')}</h2>
	<div class="grid">
		<section class="card wide">
			<h3>{t('reading.finishedPerMonth')}</h3>
			<p class="sub">{t('reading.finishedThisYear', { count: s.finishedThisYear })}</p>
			{@render bars(s.finishedPerMonth, t('reading.finishedPerMonth'))}
		</section>
		<section class="card wide">
			<h3>{t('reading.addedPerMonth')}</h3>
			{@render bars(s.addedPerMonth, t('reading.addedPerMonth'))}
		</section>
		{#if s.topAuthorsRead.length > 0}
			<section class="card">
				<h3>{t('reading.topAuthorsRead')}</h3>
				{@render tally(s.topAuthorsRead)}
			</section>
		{/if}
		{#if s.topAuthorsOwned.length > 0}
			<section class="card">
				<h3>{t('reading.topAuthorsOwned')}</h3>
				{@render tally(s.topAuthorsOwned)}
			</section>
		{/if}
		{#if s.languages.length > 0}
			<section class="card">
				<h3>{t('reading.languages')}</h3>
				{@render tally(s.languages, (code) => languageLabel(code, getLocale()))}
			</section>
		{/if}
		<section class="card">
			<h3>{t('reading.ratings')}</h3>
			{#if s.averageRating != null}
				<p class="sub">{t('reading.averageRating', { avg: s.averageRating.toFixed(1), count: s.ratedCount })}</p>
				{@render tally(
					[5, 4, 3, 2, 1].map((r) => ({ label: '★'.repeat(r), count: s.ratings[r - 1] }))
				)}
			{:else}
				<p class="sub">{t('reading.noRatings')}</p>
			{/if}
		</section>
	</div>
	<p class="footnote">{t('reading.footnote')}</p>
{/if}

<style>
	h1 {
		font-size: 1.4rem;
		margin: 0 0 1.1rem;
	}
	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(7.5rem, 1fr));
		gap: 0.6rem;
		margin-bottom: 1.75rem;
		max-width: 52rem;
	}
	.tile {
		display: flex;
		flex-direction: column;
		align-items: start;
		gap: 0.1rem;
		background: var(--card);
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 0.7rem 0.85rem;
		color: var(--muted);
		font: inherit;
		font-size: 0.8rem;
		font-weight: 400;
		text-align: left;
	}
	button.tile:hover {
		border-color: var(--accent);
		filter: none;
	}
	.big {
		font-family: Georgia, serif;
		font-size: 1.6rem;
		font-weight: 600;
		color: var(--fg);
		line-height: 1.1;
	}
	.hint {
		font-size: 0.82rem;
		color: var(--muted);
		margin: -0.5rem 0 1.5rem;
	}
	.section {
		font-size: 1.05rem;
		margin: 1rem 0 0.8rem;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
		gap: 0.9rem;
		max-width: 52rem;
	}
	.card {
		background: var(--card);
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 0.9rem 1rem;
		min-width: 0;
	}
	.card.wide {
		grid-column: 1 / -1;
	}
	h3 {
		margin: 0 0 0.2rem;
		font-size: 0.78rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--muted);
	}
	.sub {
		margin: 0 0 0.6rem;
		font-size: 0.85rem;
	}
	.chart {
		margin: 0.6rem 0 0;
	}
	.cols {
		display: grid;
		grid-template-columns: repeat(12, 1fr);
		gap: 0.35rem;
		height: 8.5rem;
		align-items: end;
	}
	.col {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: end;
		height: 100%;
		gap: 0.2rem;
		min-width: 0;
	}
	.bar {
		width: 100%;
		max-width: 2rem;
		min-height: 2px;
		background: var(--accent);
		border-radius: 3px 3px 0 0;
		opacity: 0.85;
	}
	.val {
		font-size: 0.68rem;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.mon {
		font-size: 0.66rem;
		color: var(--muted);
		white-space: nowrap;
		overflow: hidden;
		max-width: 100%;
	}
	.tally {
		list-style: none;
		margin: 0.4rem 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.tally li {
		display: grid;
		grid-template-columns: minmax(0, 8.5rem) 1fr auto;
		align-items: center;
		gap: 0.6rem;
		font-size: 0.85rem;
	}
	.t-label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.t-bar {
		height: 0.5rem;
		background: var(--bg);
		border-radius: 99px;
		overflow: hidden;
	}
	.t-bar span {
		display: block;
		height: 100%;
		background: var(--gold, var(--accent));
		border-radius: 99px;
	}
	.t-n {
		font-size: 0.78rem;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.footnote {
		font-size: 0.75rem;
		color: var(--muted);
		max-width: 52rem;
		margin-top: 1rem;
	}
	.empty {
		color: var(--muted);
		font-style: italic;
	}
</style>
