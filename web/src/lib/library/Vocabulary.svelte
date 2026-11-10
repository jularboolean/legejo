<script lang="ts">
	// A book's vocabulary on its page: for a book with a study list (Esperanto)
	// a card leading to it, and for every counted book a panel to fold out.
	import { GraduationCap, ChevronRight } from '@lucide/svelte';
	import { getLocale, t, type MessageKey } from '#lib/i18n';
	import { css, fmt, languageInSentence, loadVocab, percent, shownLevel, type Vocab } from './vocab';

	let { bookId, part }: { bookId: number; part: 'study' | 'stats' } = $props();

	let vocab = $state<Vocab | null>(null);
	$effect(() => {
		const id = bookId;
		vocab = null;
		loadVocab(id).then((v) => {
			if (bookId === id) vocab = v;
		});
	});

	const locale = $derived(getLocale());
	const n = (x: number) => fmt(x, locale);
	const known = $derived(vocab?.known_share ?? 0);
	const language = $derived(vocab ? languageInSentence(vocab.language, locale) : '');
	const level = $derived(vocab?.difficulty ? t(`vocab.level${vocab.difficulty.level}` as MessageKey) : '');

	// The coverage curve, across on a log scale: the first hundred words carry half the book.
	const W = 280;
	const H = 90;
	const path = $derived.by(() => {
		if (!vocab) return '';
		const max = Math.log10(Math.max(vocab.stems, 2));
		return vocab.curve
			.map(([k, c], i) => `${i ? 'L' : 'M'}${((Math.log10(k) / max) * W).toFixed(1)},${(H - c * H).toFixed(1)}`)
			.join(' ');
	});
	const xAt = (k: number) => (vocab ? (Math.log10(k) / Math.log10(Math.max(vocab.stems, 2))) * W : 0);
</script>

{#if vocab}
	{#if part === 'study' && vocab.study}
		<a class="study" href={`/books/${bookId}/words`}>
			<GraduationCap size={22} />
			<span class="body">
				<strong>{t('vocab.study')}</strong>
				<span class="lead">
					{#if known > 0.05}
						{t('vocab.studyKnown', { percent: percent(known) })}
						{t(`words.level.${shownLevel(known)}` as MessageKey)}
					{:else}
						{t('vocab.studyLead', { count: n(vocab.coverage['80'] ?? 0) })}
					{/if}
				</span>
				{#if known > 0.05}<span class="bar" aria-hidden="true"><span style:width={css(known)}></span></span>{/if}
			</span>
			<ChevronRight size={18} />
		</a>
	{/if}

	{#if part === 'stats'}
		<details class="vocab">
			<summary>
				{t('vocab.title')}
				<span class="teaser">· {t('vocab.teaser', { count: n(vocab.coverage['90'] ?? 0) })}</span>
				{#if vocab.difficulty}
					<span class="dots" role="img" aria-label={level} title={t('vocab.ratedTitle', { level, language })}>
						{#each [1, 2, 3, 4, 5] as i (i)}<i class:on={i <= (vocab.difficulty?.level ?? 0)}></i>{/each}
					</span>
				{/if}
			</summary>

			<div class="grid">
				<div class="facts">
					<p>{t('vocab.summary', { words: n(vocab.words), stems: n(vocab.stems), hapax: n(vocab.hapax) })}</p>
					<table>
						<tbody>
							{#each ['50', '80', '90', '95'] as k (k)}
								{#if vocab.coverage[k]}
									<tr>
										<td>{t('vocab.share', { percent: k })}</td>
										<td>{t('vocab.words', { count: n(vocab.coverage[k]) })}</td>
									</tr>
								{/if}
							{/each}
						</tbody>
					</table>
					<p class="level">
						{#if vocab.difficulty}
							{t('vocab.rated', { level, count: vocab.difficulty.compared_with, language })}
						{:else if vocab.short}
							{t('vocab.short')}
						{:else}
							{t('vocab.unrated', { language, min: vocab.rated_from, count: vocab.books_in_language })}
						{/if}
					</p>
				</div>

				<figure>
					<svg viewBox={`-2 -4 ${W + 4} ${H + 18}`} role="img" aria-label={t('vocab.curve')}>
						<line x1="0" y1={H * 0.1} x2={W} y2={H * 0.1} class="grid-line" />
						{#if vocab.coverage['90']}
							<line x1={xAt(vocab.coverage['90'])} y1={H * 0.1} x2={xAt(vocab.coverage['90'])} y2={H} class="grid-line" />
						{/if}
						<path d={path} />
						{#if vocab.coverage['90']}<circle cx={xAt(vocab.coverage['90'])} cy={H * 0.1} r="3" />{/if}
						<text x="0" y={H + 13}>1</text>
						{#if vocab.stems > 300}<text x={xAt(100)} y={H + 13} text-anchor="middle">{n(100)}</text>{/if}
						{#if vocab.stems > 3000 && xAt(1000) < W - 40}<text x={xAt(1000)} y={H + 13} text-anchor="middle">{n(1000)}</text>{/if}
						<text x={W} y={H + 13} text-anchor="end">{n(vocab.stems)}</text>
					</svg>
					<figcaption>{t('vocab.curveCaption')}</figcaption>
				</figure>
			</div>

			{#if vocab.names.length}
				<p class="charac-h">{t('vocab.names')}</p>
				<p class="note">{t('vocab.namesNote')}</p>
				<p class="charac">
					{#each vocab.names as w, i (i)}<span class="chip">{w}</span>{/each}
				</p>
			{/if}
			{#if vocab.common.length}
				<p class="charac-h">{t('vocab.common')}</p>
				<p class="note">{t('vocab.commonNote')}</p>
				<p class="charac">
					{#each vocab.common.slice(0, 10) as w, i (i)}<span class="chip">{w}</span>{/each}
				</p>
			{/if}
			<p class="charac-h">{t('vocab.characteristic')}</p>
			{#if vocab.characteristic.length}
				<p class="note">{t('vocab.characteristicNote', { language })}</p>
				<p class="charac">
					{#each vocab.characteristic.slice(0, 10) as w, i (i)}<span class="chip">{w}</span>{/each}
				</p>
			{:else}
				<p class="note">
					{t('vocab.characteristicLater', { min: vocab.distinctive_from, language, count: vocab.books_in_language })}
				</p>
			{/if}
			<p class="more"><a href={`/books/${bookId}/analysis`}>{t('analysis.link')} →</a></p>
		</details>
	{/if}
{/if}

<style>
	.study {
		display: flex;
		align-items: center;
		gap: 0.8rem;
		margin: 1rem 0 0;
		padding: 0.75rem 0.9rem;
		border: 1px solid var(--border);
		border-left: 3px solid var(--accent);
		border-radius: 8px;
		background: var(--card);
		color: var(--fg);
		text-decoration: none;
	}
	.study:hover {
		border-color: var(--accent);
	}
	.study > :global(svg:first-child) {
		color: var(--accent);
		flex: none;
	}
	.study > :global(svg:last-child) {
		color: var(--muted);
		flex: none;
	}
	.body {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		flex: 1;
		min-width: 0;
	}
	.lead {
		font-size: 0.88rem;
		color: var(--muted);
	}
	.bar {
		display: block;
		height: 4px;
		border-radius: 2px;
		background: var(--border);
		overflow: hidden;
		margin-top: 0.2rem;
	}
	.bar span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.vocab {
		margin-top: 0.9rem;
		font-size: 0.88rem;
	}
	.vocab summary {
		cursor: pointer;
		color: var(--muted);
	}
	.teaser {
		opacity: 0.85;
	}
	.dots {
		display: inline-flex;
		gap: 3px;
		margin-left: 0.4rem;
		vertical-align: middle;
	}
	.dots i {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		border: 1px solid var(--muted);
	}
	.dots i.on {
		background: var(--muted);
	}
	.grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1.2rem;
		margin-top: 0.6rem;
		align-items: start;
	}
	@media (max-width: 40rem) {
		.grid {
			grid-template-columns: 1fr;
		}
	}
	.facts p {
		margin: 0 0 0.5rem;
	}
	table {
		border-collapse: collapse;
		margin-bottom: 0.5rem;
	}
	td {
		padding: 0.1rem 1rem 0.1rem 0;
	}
	td:last-child {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.level {
		color: var(--muted);
	}
	figure {
		margin: 0;
		max-width: 22rem;
	}
	svg {
		width: 100%;
		height: auto;
		overflow: visible;
	}
	svg path {
		fill: none;
		stroke: var(--accent);
		stroke-width: 2;
	}
	svg circle {
		fill: var(--accent);
	}
	.grid-line {
		stroke: var(--border);
		stroke-dasharray: 3 3;
	}
	svg text {
		font-size: 9px;
		fill: var(--muted);
	}
	figcaption {
		font-size: 0.78rem;
		color: var(--muted);
	}
	.charac-h {
		margin: 0.9rem 0 0.1rem;
		color: var(--fg);
		font-weight: 600;
	}
	.note {
		margin: 0 0 0.35rem;
		font-size: 0.8rem;
		color: var(--muted);
	}
	.charac {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
		margin: 0;
	}
	.more {
		margin: 1rem 0 0;
	}
	.chip {
		font-size: 0.8rem;
		padding: 0.1rem 0.55rem;
		border-radius: 99px;
		border: 1px solid var(--border);
	}
</style>
