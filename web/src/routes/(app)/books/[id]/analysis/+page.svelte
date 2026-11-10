<script lang="ts">
	// A book's analysis: its style and readability, its fixed phrases, the
	// people and places it names, its grammar and its concordance. Each part
	// is a tab with an address of its own; the analysis is worked out by the
	// server the first time the page is opened.
	import { ArrowLeft } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getLocale, t, type MessageKey } from '#lib/i18n';
	import Tabs from '#lib/Tabs.svelte';
	import WordsOption from '#lib/library/WordsOption.svelte';
	import { fmt, languageInSentence } from '#lib/library/vocab';
	import {
		curvePath,
		loadAnalysis,
		loadConcordance,
		onCircle,
		ownLevel,
		OWN_NAMES,
		sortLines,
		SPREAD_LABELS,
		tagLabel,
		type Analysis,
		type Concordance,
		type LineOrder
	} from '#lib/library/analysis';

	let { data } = $props();

	const id = $derived(Number(page.params.id));
	let title = $state('');
	let analysis = $state<Analysis | null | undefined>(undefined);

	$effect(() => {
		const current = id;
		analysis = undefined;
		if (data.offline || !data.words) return;
		loadAnalysis(current).then((a) => {
			if (id === current) analysis = a;
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
	const decimal = (x: number, digits = 1) => x.toLocaleString(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits });
	const pct = (share: number) => n(Math.round(share * 100));
	const levelName = (level: number) => t(`vocab.level${level}` as MessageKey);

	type View = 'style' | 'phrases' | 'people' | 'grammar' | 'lines';
	const views = $derived<View[]>(analysis?.grammar ? ['style', 'phrases', 'people', 'grammar', 'lines'] : ['style', 'phrases', 'people', 'lines']);
	const view = $derived.by<View>(() => {
		const asked = page.url.searchParams.get('v') as View | null;
		return asked && views.includes(asked) ? asked : 'style';
	});
	const tabs = $derived(
		views.map((v) => ({ href: v === 'style' ? `/books/${id}/analysis` : `/books/${id}/analysis?v=${v}`, label: t(`analysis.tab.${v}` as MessageKey), active: view === v }))
	);

	// ------------------------------------------------------------- style
	const style = $derived(analysis?.style);
	const spreadMax = $derived(Math.max(1, ...(style?.sentence_spread ?? [])));
	const CW = 320;
	const CH = 110;
	const curve = $derived(curvePath((style?.curve ?? []).map((p) => p.lix), CW, CH));

	// ------------------------------------------------------------ people
	const NETWORK = 16;
	const NW = 440;
	const NR = 150;
	const people = $derived(analysis?.people);
	const shownNames = $derived((people?.names ?? []).slice(0, NETWORK));
	const shownLinks = $derived((people?.links ?? []).filter((l) => l.a < NETWORK && l.b < NETWORK));
	const linkMax = $derived(Math.max(1, ...shownLinks.map((l) => l.count)));
	const nameMax = $derived(Math.max(1, ...shownNames.map((x) => x.count)));
	const spot = (i: number, r = NR) => onCircle(i, shownNames.length, NW / 2, NW / 2, r);
	let chosen = $state<number | null>(null);
	const lit = (a: number, b: number) => chosen === null || chosen === a || chosen === b;
	const near = (i: number) => chosen === null || chosen === i || shownLinks.some((l) => (l.a === chosen && l.b === i) || (l.b === chosen && l.a === i));
	const presenceMax = (presence: number[]) => Math.max(1, ...presence);

	// ------------------------------------------------------- concordance
	const query = $derived(page.url.searchParams.get('q') ?? '');
	const exact = $derived(page.url.searchParams.get('exact') === 'true');
	let field = $state('');
	let fieldExact = $state(false);
	let found = $state<Concordance | null | 'loading' | 'failed'>(null);
	let order = $state<LineOrder>('at');
	$effect(() => {
		field = query;
		fieldExact = exact;
	});
	$effect(() => {
		const [book, q, ex] = [id, query, exact];
		if (view !== 'lines' || !q || !analysis) {
			found = null;
			return;
		}
		found = 'loading';
		loadConcordance(book, q, ex).then((c) => {
			if (id === book && query === q && exact === ex) found = c ?? 'failed';
		});
	});
	const linesHref = (q: string) => `/books/${id}/analysis?v=lines&q=${encodeURIComponent(q)}`;
	function search(e: SubmitEvent) {
		e.preventDefault();
		const q = field.trim();
		if (!q) return;
		goto(`${linesHref(q)}${fieldExact ? '&exact=true' : ''}`, { reset: false });
	}
	const lines = $derived(found && typeof found === 'object' ? sortLines(found.lines, order, locale) : []);
</script>

<svelte:head><title>{title ? `${t('analysis.title')} · ${title}` : t('analysis.title')}</title></svelte:head>

<a class="back" href={`/books/${id}`}><ArrowLeft size={15} /> {title || t('analysis.title')}</a>

{#if data.offline}
	<p class="muted">{t('words.offline')}</p>
{:else if !data.words}
	<WordsOption />
{:else if analysis === undefined}
	<p class="muted" aria-live="polite">{t('analysis.loading')}</p>
{:else if analysis === null}
	<p class="muted">{t('analysis.none')}</p>
{:else if style && people}
	<header>
		<h1>{t('analysis.title')}</h1>
		<p class="intro">{t('analysis.intro')}</p>
	</header>
	<Tabs {tabs} label={t('analysis.tabs')} />

	{#if view === 'style'}
		<p class="muted small">{t('analysis.measured', { words: n(style.words), sentences: n(style.sentences) })}</p>
		<dl class="tiles">
			<div>
				<dt>{t('analysis.sentence')}</dt>
				<dd>{t('analysis.sentence.value', { mean: decimal(style.sentence_mean) })}</dd>
				<p>{t('analysis.sentence.note', { median: n(Math.round(style.sentence_median)) })}</p>
			</div>
			<div>
				<dt>{t('analysis.word')}</dt>
				<dd>{t('analysis.word.value', { mean: decimal(style.word_mean) })}</dd>
				<p>{t('analysis.word.note', { percent: pct(style.long_words) })}</p>
			</div>
			<div>
				<dt>{t('analysis.spoken')}</dt>
				<dd>{pct(style.spoken)} %</dd>
				<p>{t('analysis.spoken.note')}</p>
			</div>
			<div>
				<dt>{t('analysis.variation')}</dt>
				<dd>{style.variation === null ? '–' : n(Math.round(style.variation))}</dd>
				<p>{style.variation === null ? t('analysis.variation.short') : t('analysis.variation.note')}</p>
			</div>
			<div>
				<dt>LIX</dt>
				<dd>{n(Math.round(style.lix))}</dd>
				<p>
					{t('analysis.lix.note', { level: levelName(style.level), like: t(`analysis.lix.like${style.level}` as MessageKey) })}
					{#if !['sv', 'da', 'nb', 'nn'].includes(analysis.language)}{t('analysis.lix.foreign')}{/if}
				</p>
			</div>
			{#if analysis.reach}
				{@const reach = analysis.reach}
				<div>
					<dt>{t('analysis.reach')}</dt>
					<dd>{reach.cefr ? `≈ ${reach.cefr}` : '–'}</dd>
					<p>
						{reach.need ? t('analysis.reach.need', { count: n(reach.need) }) : t('analysis.reach.beyond')}
						{t('analysis.reach.within', { a: pct(reach.within[0]?.[1] ?? 0), b: pct(reach.within[1]?.[1] ?? 0), c: pct(reach.within[2]?.[1] ?? 0) })}
						{#if reach.source}{t('analysis.reach.source', { source: reach.source })}{/if}
					</p>
				</div>
			{/if}
			{#if style.own}
				<div>
					<dt>{OWN_NAMES[style.own.name]}</dt>
					<dd>{n(Math.round(style.own.score))}</dd>
					<p>{t('analysis.own.note', { language: languageInSentence(analysis.language, locale), level: levelName(ownLevel(style.own.score)) })}</p>
				</div>
			{/if}
		</dl>

		<div class="charts">
			<figure>
				<figcaption class="h">{t('analysis.spread')}</figcaption>
				<div class="bars" role="img" aria-label={t('analysis.spread.caption')}>
					{#each style.sentence_spread as count, i (i)}
						<div class="bar" title={`${SPREAD_LABELS[i]}: ${n(count)}`}>
							<span class="share">{pct(count / Math.max(style.sentences, 1))} %</span>
							<span class="column" style:height={`${(count / spreadMax) * 100}%`}></span>
							<span class="label">{SPREAD_LABELS[i]}</span>
						</div>
					{/each}
				</div>
				<p class="caption">{t('analysis.spread.caption')}</p>
			</figure>

			<figure>
				<figcaption class="h">{t('analysis.curve')}</figcaption>
				{#if style.curve.length}
					<svg viewBox={`-24 -4 ${CW + 28} ${CH + 20}`} role="img" aria-label={t('analysis.curve.caption', { count: style.curve.length })}>
						<line x1="0" y1={CH * 0.1} x2={CW} y2={CH * 0.1} class="grid-line" />
						<line x1="0" y1={CH * 0.9} x2={CW} y2={CH * 0.9} class="grid-line" />
						<text x="-4" y={CH * 0.1 + 3} text-anchor="end">{n(Math.round(curve.max))}</text>
						<text x="-4" y={CH * 0.9 + 3} text-anchor="end">{n(Math.round(curve.min))}</text>
						<path d={curve.d} />
						<text x="0" y={CH + 13}>0 %</text>
						<text x={CW} y={CH + 13} text-anchor="end">100 %</text>
					</svg>
					<p class="caption">{t('analysis.curve.caption', { count: style.curve.length })}</p>
					<p class="trend">
						{#if style.trend > 0}
							{t('analysis.trend.harder', { points: decimal(Math.abs(style.drift), 0) })}
						{:else if style.trend < 0}
							{t('analysis.trend.easier', { points: decimal(Math.abs(style.drift), 0) })}
						{:else}
							{t('analysis.trend.even')}
						{/if}
					</p>
				{:else}
					<p class="caption">{t('analysis.curve.short')}</p>
				{/if}
			</figure>
		</div>
	{:else if view === 'phrases'}
		<p class="intro">{t('analysis.phrases.intro')}</p>
		{#if analysis.phrases.length}
			<ul class="phrases">
				{#each analysis.phrases as phrase (phrase.text)}
					<li>
						<a href={linesHref(phrase.text)} lang={analysis.language}>{phrase.text}</a>
						<span class="count">{n(phrase.count)}×</span>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="muted">{t('analysis.phrases.none')}</p>
		{/if}
	{:else if view === 'people'}
		<p class="intro">{t('analysis.people.intro')}</p>
		{#if people.names.length === 0}
			<p class="muted">{analysis.language === 'de' ? t('analysis.people.german') : t('analysis.people.none')}</p>
		{:else}
			{#if shownLinks.length}
				<h2>{t('analysis.people.network')}</h2>
				<p class="note">{t('analysis.people.networkNote')}</p>
				<svg class="network" viewBox={`0 0 ${NW} ${NW}`} role="group" aria-label={t('analysis.people.network')}>
					{#each shownLinks as link (`${link.a}-${link.b}`)}
						{@const a = spot(link.a)}
						{@const b = spot(link.b)}
						<line
							x1={a.x}
							y1={a.y}
							x2={b.x}
							y2={b.y}
							class:dim={!lit(link.a, link.b)}
							stroke-width={0.6 + (link.count / linkMax) * 5}
							stroke-opacity={0.25 + (link.count / linkMax) * 0.6}
						/>
					{/each}
					{#each shownNames as name, i (i)}
						{@const at = spot(i)}
						{@const label = spot(i, NR + 12)}
						<g
							class="node"
							class:dim={!near(i)}
							class:chosen={chosen === i}
							role="button"
							tabindex="0"
							aria-pressed={chosen === i}
							aria-label={t('analysis.people.times', { name: name.name, count: n(name.count) })}
							onclick={() => (chosen = chosen === i ? null : i)}
							onkeydown={(e) => {
								if (e.key === 'Enter' || e.key === ' ') {
									e.preventDefault();
									chosen = chosen === i ? null : i;
								}
							}}
						>
							<circle cx={at.x} cy={at.y} r={4 + Math.sqrt(name.count / nameMax) * 9} />
							<text
								x={label.x}
								y={label.y + 3}
								text-anchor={Math.abs(Math.cos(at.angle)) < 0.15 ? 'middle' : Math.cos(at.angle) > 0 ? 'start' : 'end'}
								dy={Math.sin(at.angle) > 0.9 ? 8 : Math.sin(at.angle) < -0.9 ? -6 : 0}
							>
								{name.name}
							</text>
						</g>
					{/each}
				</svg>
			{/if}

			<h2>{t('analysis.people.presence')}</h2>
			<p class="note">{t('analysis.people.presenceNote')}</p>
			<ul class="presence">
				{#each people.names as name, i (i)}
					{@const max = presenceMax(name.presence)}
					<li>
						<a href={linesHref(name.name)}>{name.name}</a>
						<span class="strip" aria-hidden="true">
							{#each name.presence as count, part (part)}<i style:opacity={count ? 0.15 + (count / max) * 0.85 : 0}></i>{/each}
						</span>
						<span class="count">{n(name.count)}×</span>
					</li>
				{/each}
			</ul>
		{/if}
	{:else if view === 'grammar' && analysis.grammar}
		<p class="intro">{analysis.grammar.source === 'pack' ? t('analysis.grammar.introPack') : t('analysis.grammar.intro')}</p>
		{#each analysis.grammar.groups as group (group.key)}
			<section class="group">
				<h2>
					{t(`analysis.grammar.${group.key}` as MessageKey)}
					<small>{t('vocab.words', { count: n(group.of) })}</small>
				</h2>
				<ul class="kinds">
					{#each group.items as item (item.tag)}
						{@const possible = item.possible ?? 0}
						<li>
							<span class="kind">{tagLabel(item.tag)}</span>
							<span class="meter" aria-hidden="true">
								<span style:width={`${(item.count / Math.max(group.of, 1)) * 100}%`}></span><span
									class="maybe"
									style:width={`${(possible / Math.max(group.of, 1)) * 100}%`}
								></span>
							</span>
							<span class="count" title={possible ? t('analysis.grammar.possible', { sure: n(item.count), possible: n(possible) }) : undefined}>
								{pct(item.count / Math.max(group.of, 1))}{#if possible}–{pct((item.count + possible) / Math.max(group.of, 1))}{/if} %
							</span>
							<span class="examples" lang={analysis.language}>
								{#each item.examples as form, k (form)}{k ? ' · ' : ''}<a href={`${linesHref(form)}&exact=true`}>{form}</a>{/each}
							</span>
						</li>
					{/each}
				</ul>
			</section>
		{/each}
	{:else if view === 'lines'}
		<p class="intro">{t('analysis.lines.intro')}</p>
		<form class="search" onsubmit={search}>
			<input type="search" bind:value={field} maxlength="80" aria-label={t('analysis.lines.label')} placeholder={t('analysis.lines.label')} lang={analysis.language} />
			<button type="submit">{t('analysis.lines.search')}</button>
			<label><input type="checkbox" bind:checked={fieldExact} /> {t('analysis.lines.exact')}</label>
		</form>
		<div aria-live="polite">
			{#if found === 'loading'}
				<p class="muted">{t('analysis.loading')}</p>
			{:else if found === 'failed'}
				<p class="muted">{t('analysis.lines.failed')}</p>
			{:else if found && found.total === 0}
				<p class="muted">{t('analysis.lines.none', { query })}</p>
			{:else if found}
				<div class="status">
					<p>
						{t('analysis.lines.total', { count: n(found.total) })}
						{#if found.total > found.lines.length}{t('analysis.lines.shown', { count: n(found.lines.length) })}{/if}
						{#if found.forms.length > 1}
							<span class="forms" lang={analysis.language}>
								{#each found.forms as [form, count] (form)}<span>{form} {n(count)}</span>{/each}
							</span>
						{/if}
					</p>
					<label>
						{t('analysis.lines.sort')}
						<select bind:value={order}>
							<option value="at">{t('analysis.lines.sort.at')}</option>
							<option value="right">{t('analysis.lines.sort.right')}</option>
							<option value="left">{t('analysis.lines.sort.left')}</option>
						</select>
					</label>
				</div>
				<p class="note">{t('analysis.lines.spoiler')}</p>
				<ol class="kwic" lang={analysis.language}>
					{#each lines as line, i (i)}
						<li>
							<span class="left"><span>{line.left}</span></span>
							<b>{line.hit}</b>
							<span class="right">{line.right}</span>
							<span class="at">{n(Math.round(line.at * 100))} %</span>
						</li>
					{/each}
				</ol>
			{/if}
		</div>
	{/if}
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
	h2 {
		margin: 1.4rem 0 0.2rem;
		font-size: 1rem;
	}
	h2 small {
		margin-left: 0.4rem;
		font-weight: 400;
		font-size: 0.82rem;
		color: var(--muted);
	}
	.intro {
		margin: 0 0 1rem;
		color: var(--muted);
		max-width: 38rem;
	}
	.small {
		font-size: 0.85rem;
		margin: 0 0 0.8rem;
	}
	.note,
	.caption {
		margin: 0 0 0.6rem;
		font-size: 0.8rem;
		color: var(--muted);
		max-width: 38rem;
	}
	.count {
		color: var(--muted);
		font-size: 0.82rem;
		font-variant-numeric: tabular-nums;
	}

	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(13.5rem, 1fr));
		gap: 0.7rem;
		margin: 0 0 1.6rem;
	}
	.tiles div {
		padding: 0.7rem 0.85rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
	}
	.tiles dt {
		font-size: 0.8rem;
		color: var(--muted);
	}
	.tiles dd {
		margin: 0.1rem 0 0.3rem;
		font-size: 1.45rem;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}
	.tiles p {
		margin: 0;
		font-size: 0.8rem;
		color: var(--muted);
	}

	.charts {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(17rem, 1fr));
		gap: 1.6rem 2rem;
		align-items: start;
	}
	figure {
		margin: 0;
		min-width: 0;
	}
	.h {
		margin-bottom: 0.5rem;
		font-weight: 600;
	}
	.bars {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: 1fr;
		gap: 0.35rem;
		height: 9.5rem;
		margin-bottom: 0.4rem;
	}
	.bar {
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		align-items: center;
		min-width: 0;
	}
	.bar .column {
		width: 100%;
		min-height: 1px;
		border-radius: 3px 3px 0 0;
		background: var(--accent);
	}
	.bar .share,
	.bar .label {
		font-size: 0.7rem;
		color: var(--muted);
		white-space: nowrap;
		font-variant-numeric: tabular-nums;
	}
	.bar .label {
		margin-top: 0.2rem;
	}
	svg {
		width: 100%;
		height: auto;
		overflow: visible;
	}
	figure svg path {
		fill: none;
		stroke: var(--accent);
		stroke-width: 2;
		stroke-linejoin: round;
	}
	.grid-line {
		stroke: var(--border);
		stroke-dasharray: 3 3;
	}
	svg text {
		font-size: 9px;
		fill: var(--muted);
	}
	.trend {
		margin: 0;
		font-size: 0.9rem;
	}

	.phrases {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
	}
	.phrases li {
		display: inline-flex;
		align-items: baseline;
		gap: 0.4rem;
		padding: 0.2rem 0.7rem;
		border: 1px solid var(--border);
		border-radius: 99px;
	}
	.phrases a {
		color: var(--fg);
		text-decoration: none;
	}
	.phrases a:hover {
		text-decoration: underline;
	}

	.network {
		display: block;
		max-width: 30rem;
		margin: 0 auto 0.5rem;
	}
	.network line {
		stroke: var(--accent);
		stroke-linecap: round;
		transition: opacity 0.15s;
	}
	.network .node {
		cursor: pointer;
		transition: opacity 0.15s;
	}
	.network .node:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}
	.network circle {
		fill: var(--card);
		stroke: var(--accent);
		stroke-width: 2;
	}
	.network .chosen circle {
		fill: var(--accent);
	}
	.network text {
		font-size: 12px;
		fill: var(--fg);
	}
	.network .dim {
		opacity: 0.12;
	}
	.presence {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.25rem;
		max-width: 40rem;
	}
	.presence li {
		display: grid;
		grid-template-columns: minmax(5rem, 9rem) 1fr 3.2rem;
		align-items: center;
		gap: 0.6rem;
	}
	.presence a {
		color: var(--fg);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.presence .count {
		text-align: right;
	}
	.strip {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: 1fr;
		gap: 1px;
		height: 0.9rem;
		border-radius: 3px;
		overflow: hidden;
		background: color-mix(in srgb, var(--border) 45%, transparent);
	}
	.strip i {
		background: var(--accent);
	}

	.kinds {
		list-style: none;
		margin: 0.4rem 0 0;
		padding: 0;
		display: grid;
		gap: 0.3rem;
		max-width: 44rem;
	}
	.kinds li {
		display: grid;
		grid-template-columns: minmax(7rem, 11rem) minmax(3rem, 8rem) 4.6rem 1fr;
		align-items: center;
		gap: 0.6rem;
	}
	.kinds .count {
		text-align: right;
	}
	.meter {
		display: flex;
		height: 0.55rem;
		border-radius: 3px;
		background: var(--border);
		overflow: hidden;
	}
	.meter span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.meter .maybe {
		opacity: 0.35;
	}
	.examples {
		font-size: 0.85rem;
		color: var(--muted);
		min-width: 0;
	}
	.examples a {
		color: inherit;
		font-style: italic;
	}
	@media (max-width: 36rem) {
		.kinds li {
			grid-template-columns: 1fr 5rem 4.6rem;
		}
		.examples {
			grid-column: 1 / -1;
			margin-bottom: 0.3rem;
		}
	}

	.search {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem 0.7rem;
		margin-bottom: 1rem;
	}
	.search input[type='search'] {
		flex: 1 1 12rem;
		max-width: 22rem;
		padding: 0.45rem 0.6rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--card);
		color: var(--fg);
		font: inherit;
	}
	.search button {
		padding: 0.45rem 0.9rem;
		border: 0;
		border-radius: 6px;
		background: var(--accent);
		color: var(--bg);
		font: inherit;
		cursor: pointer;
	}
	.search label,
	.status label {
		font-size: 0.88rem;
		color: var(--muted);
	}
	.status {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: baseline;
		gap: 0.4rem 1rem;
	}
	.status p {
		margin: 0 0 0.3rem;
	}
	.status select {
		font: inherit;
		font-size: 0.88rem;
	}
	.forms {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 0.3rem;
		margin-left: 0.4rem;
		vertical-align: baseline;
	}
	.forms span {
		font-size: 0.8rem;
		padding: 0 0.5rem;
		border: 1px solid var(--border);
		border-radius: 99px;
		color: var(--muted);
	}
	.kwic {
		list-style: none;
		margin: 0;
		padding: 0;
		font-size: 0.9rem;
	}
	.kwic li {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr) 2.6rem;
		gap: 0.45rem;
		align-items: baseline;
		padding: 0.15rem 0;
		border-bottom: 1px solid color-mix(in srgb, var(--border) 50%, transparent);
	}
	.kwic .left {
		display: flex;
		justify-content: flex-end;
		overflow: hidden;
		white-space: nowrap;
		color: var(--muted);
	}
	.kwic .right {
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		color: var(--muted);
	}
	.kwic b {
		white-space: nowrap;
	}
	.kwic .at {
		text-align: right;
		font-size: 0.72rem;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.muted {
		color: var(--muted);
	}
</style>
