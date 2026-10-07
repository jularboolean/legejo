<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { Check, Download, Headphones, Search, Sparkles } from '@lucide/svelte';
	import { formatLength } from '#lib/audio';
	import Avatar from '#lib/Avatar.svelte';
	import { defaultShelfCover } from '#lib/shelfCovers';
	import { getLocale, t } from '#lib/i18n';
	import { languageName, loadView, saveView, type ViewMode } from '#lib/library';
	import BookGrid from '#lib/library/BookGrid.svelte';
	import BookRows from '#lib/library/BookRows.svelte';
	import ViewSwitch from '#lib/library/ViewSwitch.svelte';
	import type { PublicHit } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	let query = $state(data.q);
	let debounce: ReturnType<typeof setTimeout>;
	let input = $state<HTMLInputElement>();

	$effect(() => {
		input?.focus();
	});

	// Two ways to search. The exact one answers while you type. The wider
	// one asks a language model for related terms first, which takes a
	// moment and costs the operator a little, so it waits until it is asked
	// for with Enter or the button: a longer question can be written in peace.
	let wide = $state(data.wide);
	let searching = $state(false);

	const target = (q: string, wider: boolean) =>
		q ? `/search?q=${encodeURIComponent(q)}${wider ? '&wide=1' : ''}` : wider ? '/search?wide=1' : '/search';

	function onInput() {
		clearTimeout(debounce);
		if (wide) return;
		debounce = setTimeout(() => {
			// reset: false keeps the caret in the box (and the scroll position)
			// while the results change under it.
			goto(target(query.trim(), false), { replace: true, reset: false });
		}, 250);
	}

	async function run(wider: boolean) {
		clearTimeout(debounce);
		wide = wider;
		searching = wider && query.trim() !== '';
		try {
			await goto(target(query.trim(), wider), { replace: true, reset: false });
		} finally {
			searching = false;
		}
	}

	function onSubmit(e: SubmitEvent) {
		e.preventDefault();
		run(wide);
	}

	const wideErrorText = $derived(
		data.wideError === 'too-many'
			? t('search.wider.tooMany')
			: data.wideError
				? t('search.wider.failed')
				: ''
	);

	const result = $derived(data.result);
	const nothing = $derived(
		data.q !== '' &&
			result.mine.length === 0 &&
			result.my_shelves.length === 0 &&
			result.public.length === 0 &&
			result.shelves.length === 0 &&
			result.audiobooks.length === 0
	);

	let importing = $state<Set<number>>(new Set());
	let imported = $state<Set<number>>(new Set());
	let errorMsg = $state('');

	async function importBook(id: number) {
		importing = new Set([...importing, id]);
		errorMsg = '';
		try {
			const res = await fetch(`/api/public/books/${id}/import`, { method: 'POST' });
			if (res.ok) {
				imported = new Set([...imported, id]);
				await invalidateAll();
			} else {
				const body = await res.json().catch(() => null);
				errorMsg = body?.error ?? t('public.importFailed');
			}
		} catch {
			errorMsg = t('common.network');
		} finally {
			const next = new Set(importing);
			next.delete(id);
			importing = next;
		}
	}

	function publicBookPath(hit: PublicHit): string {
		return `/public/${encodeURIComponent(hit.owner)}/${encodeURIComponent(hit.shelf_name)}/${hit.uuid}`;
	}

	/** "in {shelf} by …" cut at the shelf's name, which is shown as a link. */
	function inShelf(hit: PublicHit): [string, string] {
		const mark = '\u0000';
		const text = t('search.inShelf', { shelf: mark, by: t('public.by', { owner: hit.owner }) });
		const at = text.indexOf(mark);
		return at < 0 ? [text, ''] : [text.slice(0, at), text.slice(at + 1)];
	}

	function publicCover(hit: PublicHit): string {
		return `/api/public/books/${hit.id}/cover?size=thumb&v=${encodeURIComponent(hit.updated_at ?? hit.created_at)}`;
	}

	// Shared with the library and the shelf pages.
	let view = $state<ViewMode>(loadView());

	function setView(next: ViewMode) {
		view = next;
		saveView(next);
	}
</script>

{#snippet hitContext(hit: PublicHit)}
	{@const [before, after] = inShelf(hit)}
	{before}<a
		class="shelfpill"
		href={`/public/${encodeURIComponent(hit.owner)}/${encodeURIComponent(hit.shelf_name)}`}>{hit.shelf_name}</a
	>{after}
{/snippet}

{#snippet hitAction(hit: PublicHit)}
	{#if hit.owned}
		<span class="done"><Check size={13} /> {t('public.owned')}</span>
	{:else if imported.has(hit.id)}
		<span class="done"><Check size={13} /> {t('public.imported')}</span>
	{:else}
		<button class="ghost import" disabled={importing.has(hit.id)} onclick={() => importBook(hit.id)}>
			<Download size={13} />
			{importing.has(hit.id) ? t('public.importing') : t('public.import')}
		</button>
	{/if}
{/snippet}

<h1>{t('search.heading')}</h1>

<form class="top" onsubmit={onSubmit}>
	<div class="searchbox">
		<Search size={15} class="search-icon" aria-hidden="true" />
		<input
			type="search"
			placeholder={wide ? t('search.wider.placeholder') : t('search.placeholder')}
			bind:value={query}
			bind:this={input}
			oninput={onInput}
		/>
	</div>
	{#if data.widerSearch}
		<div class="modes" role="group" aria-label={t('search.mode')}>
			<button type="button" aria-pressed={!wide} onclick={() => run(false)}>{t('search.mode.exact')}</button>
			<button type="button" aria-pressed={wide} onclick={() => run(true)}>
				<Sparkles size={13} />
				{t('search.mode.wider')}
			</button>
		</div>
		{#if wide}
			<button type="submit" disabled={searching || query.trim() === ''}>
				{searching ? t('search.wider.searching') : t('search.wider.go')}
			</button>
		{/if}
	{/if}
	{#if result.mine.length > 0 || result.public.length > 0}
		<ViewSwitch {view} onchange={setView} />
	{/if}
</form>

{#if data.widerSearch && wide}
	{#if wideErrorText}
		<p class="error">{wideErrorText}</p>
	{:else}
		<p class="hint">
			{t('search.wider.hint')}
			<!-- What the search just made cost, for the one who pays for it. -->
			{#if data.user?.is_admin && result.usage && data.q === query.trim()}
				<span class="usage">
					{result.usage.cached
						? t('search.wider.cached')
						: t('search.wider.usage', { input: result.usage.prompt_tokens, output: result.usage.completion_tokens })}
				</span>
			{/if}
		</p>
	{/if}
{/if}

{#if errorMsg}<p class="error">{errorMsg}</p>{/if}

{#if nothing}
	<p class="empty">{t('search.none')}</p>
	{#if data.widerSearch && !wide}
		<button type="button" class="ghost widen" onclick={() => run(true)}>
			<Sparkles size={13} />
			{t('search.wider.try', { query: data.q })}
		</button>
	{/if}
{/if}

{#if result.mine.length > 0}
	<section>
		<h2>{t('search.mine')}</h2>
		{#if view === 'list'}
			<BookRows books={result.mine} showLanguage />
		{:else}
			<BookGrid books={result.mine} showLanguage />
		{/if}
	</section>
{/if}

{#if result.my_shelves.length > 0}
	<section>
		<h2>{t('sidebar.shelves')}</h2>
		<div class="shelfgrid">
			{#each result.my_shelves as shelf (shelf.id)}
				<a class="shelfcard" href={`/shelves/${shelf.id}`}>
					<div class="shelfcover">
						<img
							src={shelf.has_cover ? `/api/shelves/${shelf.id}/cover` : defaultShelfCover(shelf.id)}
							alt=""
							loading="lazy"
						/>
					</div>
					<div class="shelfmeta">
						<span class="name">{shelf.name}</span>
						<span class="sub">{t('shelf.books', { count: shelf.book_count })}</span>
					</div>
				</a>
			{/each}
		</div>
	</section>
{/if}

{#if result.audiobooks.length > 0}
	<section>
		<h2>{t('sidebar.audiobooks')}</h2>
		<div class="shelfgrid">
			{#each result.audiobooks as book (book.id)}
				<a class="shelfcard" href={`/audiobooks/${book.id}`}>
					<div class="shelfcover">
						{#if book.has_cover}
							<img
								src={`/api/audiobooks/${book.id}/cover?v=${encodeURIComponent(book.updated_at ?? '')}`}
								alt=""
								loading="lazy"
							/>
						{:else}
							<Headphones size={20} strokeWidth={1.5} />
						{/if}
					</div>
					<div class="shelfmeta">
						<span class="name">{book.title || t('audio.untitled')}</span>
						<span class="sub">
							{[book.author, formatLength(book.seconds), languageName(book.language, getLocale())]
								.filter(Boolean)
								.join(' · ')}
						</span>
					</div>
				</a>
			{/each}
		</div>
	</section>
{/if}

{#if result.public.length > 0}
	<section>
		<h2>{t('search.public')}</h2>
		{#if view === 'list'}
			<BookRows
				books={result.public}
				showLanguage
				href={publicBookPath}
				cover={publicCover}
				own={false}
				context={hitContext}
				action={hitAction}
			/>
		{:else}
			<div class="grid">
				{#each result.public as hit (hit.id)}
					<div class="card">
						<a class="cover" href={publicBookPath(hit)}>
							{#if hit.has_cover}
								<img src={publicCover(hit)} alt={hit.title} loading="lazy" />
							{:else}
								<span class="placeholder">{hit.title}</span>
							{/if}
						</a>
						<a class="title" href={publicBookPath(hit)} title={hit.title}>{hit.title}</a>
						{#if hit.author}<span class="sub">{hit.author}</span>{/if}
						{#if languageName(hit.language, getLocale())}
							<span class="sub">{languageName(hit.language, getLocale())}</span>
						{/if}
						<span class="context">{@render hitContext(hit)}</span>
						{@render hitAction(hit)}
					</div>
				{/each}
			</div>
		{/if}
	</section>
{/if}

{#if result.shelves.length > 0}
	<section>
		<h2>{t('search.publicShelves')}</h2>
		<div class="shelfgrid">
			{#each result.shelves as shelf (shelf.id)}
				<a
					class="shelfcard"
					href={`/public/${encodeURIComponent(shelf.owner)}/${encodeURIComponent(shelf.name)}`}
				>
					<div class="shelfcover">
						<img
							src={shelf.has_cover ? `/api/public/shelves/${shelf.id}/cover` : defaultShelfCover(shelf.id)}
							alt=""
							loading="lazy"
						/>
					</div>
					<div class="shelfmeta">
						<span class="name">{shelf.name}</span>
						<span class="sub subrow">
							<Avatar userId={shelf.owner_id} hasAvatar={shelf.owner_has_avatar} size={16} alt="" />
							{t('public.by', { owner: shelf.owner })} · {shelf.book_count}
						</span>
					</div>
				</a>
			{/each}
		</div>
	</section>
{/if}

<style>
	h1 {
		font-size: 1.4rem;
		margin: 0 0 1rem;
	}
	.top {
		display: flex;
		flex-direction: row;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.75rem;
		max-width: none;
		margin-bottom: 1.75rem;
	}
	.top :global(.seg) {
		margin-left: auto;
	}
	.modes {
		display: inline-flex;
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
	}
	.modes button {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		background: none;
		color: var(--muted);
		border: none;
		border-radius: 0;
		font-size: 0.85rem;
		font-weight: 500;
		letter-spacing: 0;
		padding: 0.4rem 0.8rem;
	}
	.modes button[aria-pressed='true'] {
		background: var(--accent);
		color: var(--bg);
	}
	.hint {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.35rem;
		margin: -1rem 0 1.5rem;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.usage {
		margin-left: 0.4rem;
		font-size: 0.78rem;
		font-variant-numeric: tabular-nums;
	}
	.widen {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.88rem;
	}
	.searchbox {
		position: relative;
		flex: 1;
		max-width: 28rem;
	}
	.searchbox :global(.search-icon) {
		position: absolute;
		left: 0.7rem;
		top: 50%;
		transform: translateY(-50%);
		color: var(--muted);
		pointer-events: none;
	}
	.searchbox input {
		padding-left: 2.1rem;
	}
	.empty {
		color: var(--muted);
	}
	section {
		margin-bottom: 2.25rem;
	}
	h2 {
		font-size: 1.05rem;
		margin: 0 0 0.75rem;
		padding-bottom: 0.35rem;
		border-bottom: 1px solid var(--border);
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr));
		gap: 1.25rem;
	}
	.card {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		color: var(--fg);
	}
	.cover {
		aspect-ratio: 2 / 3;
		border-radius: 3px 6px 6px 3px;
		overflow: hidden;
		background: var(--card);
		border: 1px solid var(--border);
		display: flex;
		box-shadow: var(--shadow);
	}
	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.placeholder {
		margin: auto;
		padding: 0.75rem;
		text-align: center;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.title {
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--fg);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	a.title:hover {
		color: var(--accent);
		text-decoration: none;
	}
	.sub {
		font-size: 0.8rem;
		color: var(--muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.context {
		font-size: 0.75rem;
		color: var(--gold);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.shelfpill {
		display: inline-block;
		padding: 0 0.5rem;
		border: 1px solid var(--gold);
		border-radius: 99px;
		color: var(--gold);
		font-weight: 600;
	}
	.shelfpill:hover {
		text-decoration: none;
		background: var(--card);
	}
	.import {
		font-size: 0.78rem;
		padding: 0.3rem 0.6rem;
		justify-content: center;
		margin-top: 0.15rem;
		white-space: nowrap;
	}
	.done {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.3rem;
		font-size: 0.8rem;
		color: var(--accent);
		padding: 0.3rem 0;
		white-space: nowrap;
	}
	.shelfgrid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
		gap: 1rem;
	}
	.shelfcard {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
		padding: 0.5rem 0.75rem;
		color: var(--fg);
		box-shadow: var(--shadow);
	}
	.shelfcard:hover {
		text-decoration: none;
		border-color: var(--accent);
	}
	.shelfcover {
		width: 3.2rem;
		height: 3.2rem;
		border-radius: 6px;
		overflow: hidden;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg);
		color: var(--gold);
		border: 1px solid var(--border);
	}
	.shelfcover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.shelfmeta {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.subrow {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
	}
	.name {
		font-weight: 600;
		font-size: 0.95rem;
	}
</style>
