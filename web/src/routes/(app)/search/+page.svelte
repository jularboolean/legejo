<script lang="ts">
	import Elsewhere from './Elsewhere.svelte';
	import { goto, invalidateAll } from '$app/navigation';
	import { BookOpen, Check, Download, Headphones, Search } from '@lucide/svelte';
	import { formatLength } from '#lib/audio';
	import Avatar from '#lib/Avatar.svelte';
	import { defaultShelfCover } from '#lib/shelfCovers';
	import { getLocale, t } from '#lib/i18n';
	import { languageName, loadView, saveView, type ViewMode } from '#lib/library';
	import BookGrid from '#lib/library/BookGrid.svelte';
	import BookRows from '#lib/library/BookRows.svelte';
	import ViewSwitch from '#lib/library/ViewSwitch.svelte';
	import type { PublicHit, TextPassage } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	let query = $state(data.q);
	let debounce: ReturnType<typeof setTimeout>;
	let input = $state<HTMLInputElement>();

	$effect(() => {
		input?.focus();
	});

	function target(q: string, inText: boolean): string {
		const params = new URLSearchParams();
		if (q) params.set('q', q);
		if (inText) params.set('in', 'text');
		const rest = params.toString();
		return rest ? `/search?${rest}` : '/search';
	}

	function onInput() {
		clearTimeout(debounce);
		debounce = setTimeout(() => {
			// reset: false keeps the caret in the box (and the scroll position)
			// while the results change under it.
			goto(target(query.trim(), data.inText), { replace: true, reset: false });
		}, 250);
	}

	function setInText(on: boolean) {
		clearTimeout(debounce);
		goto(target(query.trim(), on), { replace: true, reset: false });
		input?.focus();
	}

	const MARK_START = '\uE000';
	const MARK_END = '\uE001';

	/** A passage cut into plain and matched stretches. */
	function stretches(text: string): { text: string; mark: boolean }[] {
		const out: { text: string; mark: boolean }[] = [];
		for (const piece of text.split(MARK_START)) {
			const end = piece.indexOf(MARK_END);
			if (end < 0) {
				if (piece) out.push({ text: piece, mark: false });
				continue;
			}
			out.push({ text: piece.slice(0, end), mark: true });
			if (end + 1 < piece.length) out.push({ text: piece.slice(end + 1), mark: false });
		}
		return out;
	}

	/** The reader looks for one string: the first phrase in quotes, else the
	 *  query when it is one word, else the first word that matched in this
	 *  passage. */
	function readerLink(bookId: number, passage: TextPassage): string {
		const phrase = data.q.match(/"([^"]+)"?/)?.[1]?.trim();
		const words = data.q.split(/\s+/).filter(Boolean);
		const marked = stretches(passage.text).find((s) => s.mark)?.text;
		const find = phrase || (words.length === 1 ? words[0] : (marked ?? words[0] ?? ''));
		return `/books/${bookId}/read?q=${encodeURIComponent(find)}`;
	}

	const result = $derived(data.result);
	const text = $derived(data.text);
	let elsewhere = $state(0);
	const nothing = $derived(
		data.q !== '' &&
			elsewhere === 0 &&
			(text?.books.length ?? 0) === 0 &&
			(text?.shared.length ?? 0) === 0 &&
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

{#snippet textPassage(passage: TextPassage, href: string, title: string)}
	<a class="passage" {href} {title}>
		<span class="where">
			<BookOpen size={12} aria-hidden="true" />
			{[passage.title, t('search.textAt', { percent: Math.round(passage.percent) })].filter(Boolean).join(' · ')}
		</span>
		<span class="quote"
			>{#each stretches(passage.text) as s, j (j)}{#if s.mark}<mark>{s.text}</mark>{:else}{s.text}{/if}{/each}</span
		>
	</a>
{/snippet}

<h1>{t('search.heading')}</h1>

<div class="top">
	<div class="searchbox">
		<Search size={15} class="search-icon" aria-hidden="true" />
		<input
			type="search"
			placeholder={data.inText ? t('search.textPlaceholder') : t('search.placeholder')}
			bind:value={query}
			bind:this={input}
			oninput={onInput}
		/>
	</div>
	{#if data.fulltext}
		<label class="intext">
			<input type="checkbox" checked={data.inText} onchange={(e) => setInText(e.currentTarget.checked)} />
			{t('search.inText')}
		</label>
	{/if}
	{#if result.mine.length > 0 || result.public.length > 0}
		<span class="view"><ViewSwitch {view} onchange={setView} /></span>
	{/if}
</div>
{#if data.fulltext && data.inText}<p class="note phrases">{t('search.textPhrases')}</p>{/if}

{#if errorMsg}<p class="error">{errorMsg}</p>{/if}

{#if nothing}
	<p class="empty">{t('search.none')}</p>
	{#if data.librarian}
		<a class="ask" href={`/librarian?q=${encodeURIComponent(data.q)}`}>{t('librarian.askInstead')}</a>
	{/if}
{/if}

{#if text}
	{#if text.pending > 0}<p class="note">{t('search.textPending', { count: text.pending })}</p>{/if}
	{#if text.truncated}<p class="note">{t('search.textTruncated')}</p>{/if}
	{#if text.books.length > 0}
		<section>
			<h2>{t('search.mine')}</h2>
			<ul class="texthits">
				{#each text.books as hit (hit.book.id)}
					<li>
						<a class="textcover" href={`/books/${hit.book.id}`}>
							{#if hit.book.has_cover}
								<img
									src={`/api/books/${hit.book.id}/cover?size=thumb&v=${encodeURIComponent(hit.book.updated_at ?? hit.book.created_at)}`}
									alt=""
									loading="lazy"
								/>
							{/if}
						</a>
						<div class="textbody">
							<a class="title" href={`/books/${hit.book.id}`}>{hit.book.title}</a>
							<span class="sub">
								{[hit.book.author, t('search.textHits', { count: hit.hits })].filter(Boolean).join(' · ')}
							</span>
							{#each hit.passages as passage, i (i)}
								{@render textPassage(passage, readerLink(hit.book.id, passage), t('search.textOpen'))}
							{/each}
						</div>
					</li>
				{/each}
			</ul>
		</section>
	{/if}
	{#if text.shared.length > 0}
		<section>
			<h2>{t('search.public')}</h2>
			<ul class="texthits">
				{#each text.shared as hit (hit.book.id)}
					<li>
						<a class="textcover" href={publicBookPath(hit.book)}>
							{#if hit.book.has_cover}
								<img src={publicCover(hit.book)} alt="" loading="lazy" />
							{/if}
						</a>
						<div class="textbody">
							<a class="title" href={publicBookPath(hit.book)}>{hit.book.title}</a>
							<span class="sub">
								{[hit.book.author, t('search.textHits', { count: hit.hits })].filter(Boolean).join(' · ')}
							</span>
							<span class="context">{@render hitContext(hit.book)}</span>
							{#each hit.passages as passage, i (i)}
								{@render textPassage(passage, publicBookPath(hit.book), t('search.textShared'))}
							{/each}
							<div>{@render hitAction(hit.book)}</div>
						</div>
					</li>
				{/each}
			</ul>
		</section>
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

{#if !data.inText}<Elsewhere q={data.q} bind:found={elsewhere} />{/if}

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
	/* The search box, the box for searching the text beside it, and the view
	   switch at the far end; on a narrow screen the text box wraps below. */
	.top {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.75rem 1rem;
		margin-bottom: 1.75rem;
	}
	.searchbox {
		position: relative;
		flex: 1 1 18rem;
		max-width: 28rem;
	}
	.view {
		margin-left: auto;
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
	.intext {
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		font-size: 0.9rem;
		white-space: nowrap;
		cursor: pointer;
	}
	.intext input {
		width: auto;
		margin: 0;
	}
	.note {
		color: var(--muted);
		font-size: 0.85rem;
	}
	.phrases {
		margin: -1.1rem 0 1.5rem;
	}
	.texthits {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}
	.texthits li {
		display: flex;
		gap: 0.9rem;
		align-items: flex-start;
	}
	.textcover {
		flex-shrink: 0;
		width: 3.4rem;
		aspect-ratio: 2 / 3;
		border-radius: 2px 4px 4px 2px;
		overflow: hidden;
		background: var(--card);
		border: 1px solid var(--border);
		box-shadow: var(--shadow);
	}
	.textcover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.textbody {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		min-width: 0;
		flex: 1;
	}
	.passage {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		padding: 0.45rem 0.65rem;
		border-left: 2px solid var(--border);
		color: var(--fg);
		font-size: 0.9rem;
		line-height: 1.45;
	}
	.passage:hover {
		text-decoration: none;
		border-left-color: var(--accent);
		background: var(--card);
	}
	.where {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.75rem;
		color: var(--muted);
	}
	.quote {
		overflow-wrap: anywhere;
	}
	.quote mark {
		background: color-mix(in srgb, var(--gold) 30%, transparent);
		color: inherit;
		border-radius: 2px;
		padding: 0 0.1em;
	}
	.ask {
		font-size: 0.9rem;
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
