<script lang="ts">
	// Every highlight and note in one's own books, together: book by book, in
	// the order they stand in the book, with a search over them all. A note
	// can be written or changed here, a highlight removed, and each opens the
	// book at its place.
	import { page } from '$app/state';
	import { Download, Highlighter, NotebookPen, Pencil, Search, Share2, Trash2 } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import ShareQuote from '#lib/ShareQuote.svelte';
	import { cover } from '#lib/cover';
	import { t } from '#lib/i18n';
	import { byBook, placeLine, search, type Note } from '#lib/notes';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	// svelte-ignore state_referenced_locally
	let notes = $state<Note[]>(data.notes);
	let query = $state('');
	let withNotesOnly = $state(false);

	const shown = $derived(byBook(search(notes, query, withNotesOnly)));
	// The book asked for in the address (#book-12), as from its own page.
	// Not :target, which the app's own navigation does not set.
	const wanted = $derived(page.url.hash.slice(1));
	const count = $derived(shown.reduce((n, b) => n + b.notes.length, 0));

	let editing = $state<number | null>(null);
	let draft = $state('');
	let saving = $state(false);
	let failed = $state<number | null>(null);
	let removing = $state<Note | null>(null);
	let sharing = $state<Note | null>(null);

	function edit(n: Note) {
		editing = n.id;
		draft = n.note ?? '';
		failed = null;
	}

	async function save(n: Note) {
		saving = true;
		failed = null;
		try {
			const res = await fetch(`/api/annotations/${n.id}`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ note: draft })
			});
			if (!res.ok) throw new Error(String(res.status));
			const saved = await res.json();
			notes = notes.map((x) => (x.id === n.id ? { ...x, note: saved.note, updated_at: saved.updated_at } : x));
			editing = null;
		} catch {
			failed = n.id;
		} finally {
			saving = false;
		}
	}

	async function remove() {
		const n = removing;
		removing = null;
		if (!n) return;
		const res = await fetch(`/api/annotations/${n.id}`, { method: 'DELETE' }).catch(() => null);
		if (res?.ok) notes = notes.filter((x) => x.id !== n.id);
	}

	const coverUrl = (b: { bookId: number; changedAt: string }) =>
		`/api/books/${b.bookId}/cover?size=thumb&v=${encodeURIComponent(b.changedAt)}`;
	// The card is printed large: the whole cover, not the thumbnail.
	const fullCover = (n: Note) =>
		n.book_has_cover ? `/api/books/${n.book_id}/cover?v=${encodeURIComponent(n.book_changed_at)}` : null;
</script>

<div class="heading">
	<h1>{t('notes.heading')} <span class="count">{notes.length}</span></h1>
	{#if notes.length > 0}
		<a class="button ghost export" href="/api/annotations/export" download title={t('notes.export')} aria-label={t('notes.export')}><Download size={16} /></a>
	{/if}
</div>

{#if notes.length === 0}
	<p class="empty">{t('notes.empty')}</p>
{:else}
	<div class="tools">
		<label class="find">
			<Search size={14} aria-hidden="true" />
			<input type="search" bind:value={query} placeholder={t('notes.search')} aria-label={t('notes.search')} />
		</label>
		<label class="only">
			<input type="checkbox" bind:checked={withNotesOnly} />
			{t('notes.withNotes')}
		</label>
	</div>

	{#if count === 0}
		<p class="empty">{t('notes.noMatch')}</p>
	{/if}

	{#each shown as book (book.bookId)}
		<section class="book" id={`book-${book.bookId}`} class:arrived={wanted === `book-${book.bookId}`}>
			<a class="book-head" href={`/books/${book.bookId}`}>
				<span class="thumb">
					{#if book.hasCover}<img use:cover src={coverUrl(book)} alt="" loading="lazy" />{/if}
				</span>
				<span class="book-name">
					<span class="title">{book.title}</span>
					{#if book.author}<span class="author">{book.author}</span>{/if}
				</span>
			</a>
			<ol>
				{#each book.notes as n (n.id)}
					{@const where = placeLine(n, (p) => t('notes.page', { page: p }))}
					<li class="note" style:--mark={n.color ?? undefined}>
						<!-- The book's words, as a quotation; the reader's own below, set apart. -->
						<figure class="passage">
							<blockquote>{n.text}</blockquote>
							{#if where}<figcaption class="where">{where}</figcaption>{/if}
						</figure>
						{#if editing === n.id}
							<textarea bind:value={draft} rows="3" aria-label={t('notes.note')}></textarea>
							<div class="actions">
								<button type="button" disabled={saving} onclick={() => save(n)}>{t('edit.save')}</button>
								<button type="button" class="ghost" disabled={saving} onclick={() => (editing = null)}>{t('common.cancel')}</button>
								{#if failed === n.id}<span class="error">{t('edit.saveFailed')}</span>{/if}
							</div>
						{:else}
							{#if n.note}
								<div class="own">
									<NotebookPen size={14} aria-hidden="true" />
									<p><span class="visually-hidden">{t('notes.yours')}: </span>{n.note}</p>
								</div>
							{/if}
							<div class="actions">
								{#if n.source === 'kobo'}<span class="tag">Kobo</span>{/if}
								{#if n.cfi}
									<a href={`/books/${n.book_id}/read?at=${n.id}&from=notes`}><Highlighter size={13} /> {t('notes.inBook')}</a>
								{/if}
								<button type="button" class="link" onclick={() => edit(n)}><Pencil size={13} /> {n.note ? t('notes.editNote') : t('notes.addNote')}</button>
								<button type="button" class="link" onclick={() => (sharing = n)}><Share2 size={13} /> {t('share.button')}</button>
								<button type="button" class="link quiet" onclick={() => (removing = n)} aria-label={t('notes.remove')}><Trash2 size={13} /></button>
							</div>
						{/if}
					</li>
				{/each}
			</ol>
		</section>
	{/each}
{/if}

<ShareQuote
	open={sharing !== null}
	quote={sharing?.text ?? ''}
	note={sharing?.note ?? null}
	title={sharing?.book_title ?? ''}
	author={sharing?.book_author ?? null}
	coverUrl={sharing ? fullCover(sharing) : null}
	color={sharing?.color}
	onclose={() => (sharing = null)}
/>

<ConfirmDialog
	open={removing !== null}
	title={t('notes.removeTitle')}
	message={removing ? removing.text : ''}
	confirmLabel={t('notes.remove')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={remove}
	oncancel={() => (removing = null)}
/>

<style>
	.heading {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		margin-bottom: 1.1rem;
	}
	h1 {
		font-size: 1.4rem;
		margin: 0;
	}
	.count {
		color: var(--muted);
		font-family: 'Inter Variable', system-ui, sans-serif;
		font-weight: 400;
		font-size: 0.95rem;
		margin-left: 0.3rem;
	}
	.export {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		padding: 0.4rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--fg);
		text-decoration: none;
	}
	.empty {
		color: var(--muted);
	}
	.tools {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.6rem 1.2rem;
		margin-bottom: 1.5rem;
	}
	.find {
		flex: 1;
		min-width: 14rem;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0 0.7rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
		color: var(--muted);
	}
	.find input {
		flex: 1;
		border: none;
		background: none;
		padding: 0.5rem 0;
		color: var(--fg);
		font: inherit;
		outline: none;
	}
	.only {
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		font-size: 0.88rem;
		color: var(--muted);
	}
	.book {
		margin-bottom: 2rem;
		/* Reached from the book's page (#book-12): clear of the bar above,
		   and picked out for a moment. */
		scroll-margin-top: 4.5rem;
	}
	.book.arrived .book-head .title {
		color: var(--accent);
	}
	:global(:root[data-motion='on']) .book.arrived {
		animation: arrived 1.6s ease-out;
	}
	@keyframes arrived {
		from {
			background: color-mix(in srgb, var(--accent) 12%, transparent);
			box-shadow: 0 0 0 0.6rem color-mix(in srgb, var(--accent) 12%, transparent);
			border-radius: 8px;
		}
	}
	.book-head {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		color: var(--fg);
		text-decoration: none;
		margin-bottom: 0.6rem;
	}
	.thumb {
		flex-shrink: 0;
		width: 2.4rem;
		aspect-ratio: 2 / 3;
		border-radius: 2px 4px 4px 2px;
		overflow: hidden;
		background: var(--border);
	}
	.thumb img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.book-name {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.title {
		font-weight: 600;
	}
	.author {
		font-size: 0.85rem;
		color: var(--muted);
	}
	ol {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	.note {
		padding: 0.8rem 0.95rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
	}
	/* The book's words: in a book face, behind a bar in the highlight's
	   colour, as they are marked in the reader. */
	.passage {
		margin: 0;
		padding-left: 0.8rem;
		border-left: 3px solid var(--mark, var(--gold));
	}
	blockquote {
		margin: 0;
		font-family: Georgia, 'Iowan Old Style', serif;
		font-size: 1.02rem;
		line-height: 1.5;
	}
	.where {
		margin-top: 0.25rem;
		font-size: 0.76rem;
		color: var(--muted);
	}
	/* One's own words: in the interface's face, on a tint of its colour,
	   with a pen before them. */
	.own {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		margin-top: 0.65rem;
		padding: 0.5rem 0.7rem;
		border-radius: 6px;
		background: color-mix(in srgb, var(--accent) 9%, transparent);
		color: var(--fg);
		font-size: 0.92rem;
	}
	.own :global(svg) {
		flex-shrink: 0;
		margin-top: 0.2rem;
		color: var(--accent);
	}
	.own p {
		margin: 0;
		white-space: pre-wrap;
	}
	.visually-hidden {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}
	textarea {
		width: 100%;
		margin-top: 0.5rem;
		font: inherit;
		font-size: 0.92rem;
		padding: 0.5rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg);
		color: var(--fg);
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem 1rem;
		margin-top: 0.5rem;
		font-size: 0.82rem;
	}
	.actions a,
	.link {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
	}
	.link {
		padding: 0;
		background: none;
		border: none;
		color: var(--accent);
		font: inherit;
		font-weight: 500;
		letter-spacing: 0;
	}
	.link:hover {
		filter: none;
		text-decoration: underline;
	}
	.quiet {
		color: var(--muted);
		margin-left: auto;
	}
	.tag {
		font-size: 0.72rem;
		padding: 0.05rem 0.4rem;
		border-radius: 99px;
		border: 1px solid var(--border);
		color: var(--muted);
	}
</style>
