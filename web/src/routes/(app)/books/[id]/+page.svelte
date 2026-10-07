<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { ArrowLeft, BookMarked, BookOpen, Bookmark, BookmarkCheck, Download, FileWarning, Layers, Pencil, Send, TabletSmartphone, Tag, Wrench } from '@lucide/svelte';
	import { canRepair, fixedList, issueText, needsAttention } from '#lib/health';
	import { getLocale, t } from '#lib/i18n';
	import { splitAuthors } from '#lib/library/authors';
	import { fedHost } from '#lib/fed';
	import { licenseName, licenseUrl } from '#lib/license';
	import { renderMarkdown } from '#lib/markdown';
	import { formatPublished } from '#lib/pubdate';
	import Rating from '#lib/Rating.svelte';
	import { localPercent } from '#lib/reader/progress';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const book = $derived(data.book);

	let kindleState = $state<'idle' | 'sending' | 'sent'>('idle');
	let kindleError = $state('');
	async function sendToKindle() {
		kindleState = 'sending';
		kindleError = '';
		try {
			const res = await fetch(`/api/books/${book.id}/kindle`, { method: 'POST' });
			if (res.ok) {
				kindleState = 'sent';
				return;
			}
			const code = (await res.json().catch(() => null))?.error;
			kindleError =
				code === 'too-large' ? t('kindle.tooLarge') : code === 'too-many' ? t('kindle.tooMany') : t('kindle.failed');
		} catch {
			kindleError = t('common.network');
		}
		kindleState = 'idle';
	}
	const descriptionHtml = $derived(book.description ? renderMarkdown(book.description) : '');

	function seriesLabel(name: string, index: number | null): string {
		if (index == null) return name;
		const n = Number.isInteger(index) ? String(index) : String(index);
		return `${name} #${n}`;
	}

	// The server reports progress across devices; a server without the progress
	// API leaves the reader's own copy in this browser.
	const progress = $derived(book.progress_percent ?? localPercent(book.id));

	// Shown at once; the server's value takes over when the reload lands.
	// Want-to-read: shown at once, confirmed by the reload.
	let pendingWant = $state<boolean | undefined>(undefined);
	const wanted = $derived(pendingWant ?? book.want_to_read);

	async function toggleWant() {
		const want = !wanted;
		pendingWant = want;
		try {
			const res = await fetch(`/api/books/${book.id}/want`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ want })
			});
			if (res.ok) await invalidateAll();
		} finally {
			pendingWant = undefined;
		}
	}

	let pendingRating = $state<number | null | undefined>(undefined);
	const rating = $derived(pendingRating !== undefined ? pendingRating : book.rating);

	async function rate(value: number | null) {
		pendingRating = value;
		try {
			const res = await fetch(`/api/books/${book.id}/rating`, {
				method: 'PUT',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ rating: value })
			});
			if (res.ok) await invalidateAll();
		} finally {
			pendingRating = undefined;
		}
	}

	let restoring = $state(false);

	async function sendToKobo() {
		restoring = true;
		try {
			const res = await fetch(`/api/books/${book.id}/kobo-restore`, { method: 'POST' });
			if (res.ok) await invalidateAll();
		} finally {
			restoring = false;
		}
	}

	// The health check: what is wrong with the file, and what a repair would mend.
	// Issues that call for something to be done are shown up front; the rest
	// is detail, folded away.
	const health = $derived(book.health);
	const attention = $derived((health?.issues ?? []).filter(needsAttention));
	const details = $derived((health?.issues ?? []).filter((i) => !needsAttention(i)));
	const repairable = $derived(attention.some((i) => canRepair(i, book)));
	let repairing = $state(false);
	let repairFailed = $state(false);

	async function repairFile() {
		repairing = true;
		repairFailed = false;
		try {
			const res = await fetch(`/api/books/${book.id}/repair`, { method: 'POST' });
			if (res.ok) await invalidateAll();
			else repairFailed = true;
		} catch {
			repairFailed = true;
		} finally {
			repairing = false;
		}
	}

	function formatSize(bytes: number): string {
		if (bytes >= 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + ' MB';
		return Math.max(1, Math.round(bytes / 1024)) + ' kB';
	}
</script>

<a class="back" href="/"><ArrowLeft size={13} /> {t('book.back')}</a>

<div class="detail">
	<div class="side">
		<div class="cover">
			{#if book.has_cover}
				<img src={`/api/books/${book.id}/cover?v=${encodeURIComponent(book.updated_at ?? book.created_at)}`} alt={book.title} />
			{:else}
				<span class="placeholder">{book.title}</span>
			{/if}
		</div>
		{#if progress !== null}
			<div
				class="progress"
				role="progressbar"
				aria-valuemin="0"
				aria-valuemax="100"
				aria-valuenow={Math.round(progress * 100)}
				aria-label={t('book.progress', { percent: Math.round(progress * 100) })}
				title={t('book.progress', { percent: Math.round(progress * 100) })}
			>
				<span style:width={`${Math.round(progress * 100)}%`}></span>
			</div>
		{/if}
		{#if book.format !== 'epub'}<p class="elsewhere">{t('book.readElsewhere', { format: book.format.toUpperCase() })}</p>{/if}
		{#if book.format === 'epub'}
			<a class="read" href={`/books/${book.id}/read`}>
				<BookOpen size={15} />
				{progress !== null ? t('book.continue') : t('book.read')}
			</a>
		{/if}
		<a class="download" href={`/api/books/${book.id}/file`}>
			<Download size={14} />
			{t('book.download', { format: book.format.toUpperCase() })}
		</a>
		{#if data.sendToKindle && book.format !== 'cbz'}
			<button type="button" class="want" disabled={kindleState === 'sending'} onclick={sendToKindle}>
				<Send size={14} />
				{kindleState === 'sending' ? t('kindle.sending') : kindleState === 'sent' ? t('kindle.sent') : t('kindle.send')}
			</button>
			{#if kindleError}<p class="kindle-error" role="alert">{kindleError}</p>{/if}
		{/if}
		<button type="button" class="want" class:on={wanted} aria-pressed={wanted} onclick={toggleWant}>
			{#if wanted}<BookmarkCheck size={14} />{:else}<Bookmark size={14} />{/if}
			{wanted ? t('book.wanted') : t('book.want')}
		</button>
	</div>

	<div class="info">
		<div class="headline">
			<div>
				<h1>{book.title}</h1>
				{#if book.author}
					{@const people = splitAuthors(book.author)}
					<p class="author">
						{#if people.length === 0}
							{book.author}
						{:else}
							{#each people as person, i (person.key)}
								{#if i > 0}<span class="sep">, </span>{/if}
								<a href={`/?author=${encodeURIComponent(person.key)}`} title={t('authors.showBooks', { name: person.name })}>{person.name}</a>
							{/each}
						{/if}
					</p>
				{/if}
				{#if book.series}
					<p class="series">
						<a href={`/series/${encodeURIComponent(book.series)}`} title={t('book.series')}>
							<Layers size={13} />
							{seriesLabel(book.series, book.series_index)}
						</a>
					</p>
				{/if}
				<div class="rating"><Rating value={rating} onchange={rate} /></div>
			</div>
			<a class="edit-btn" href={`/books/${book.id}/edit`}><Pencil size={13} /> {t('book.edit')}</a>
		</div>

		{#if attention.length > 0}
			<section class="health" aria-label={t('health.heading')}>
				<h2><FileWarning size={14} /> {t('health.title')}</h2>
				<ul>
					{#each attention as issue (issue.code)}
						<li>
							{issueText(issue)}
							{#if !canRepair(issue, book) && (issue.code === 'no_language' || issue.code === 'no_cover')}
								<a href={`/books/${book.id}/edit`}>{t('health.toEdit')}</a>
							{/if}
						</li>
					{/each}
				</ul>
				{#if repairable}
					<button type="button" class="ghost" disabled={repairing} onclick={repairFile}>
						<Wrench size={13} />
						{repairing ? t('health.repairing') : t('health.repair')}
					</button>
				{/if}
				{#if repairFailed}<p class="error">{t('health.repairFailed')}</p>{/if}
			</section>
		{/if}

		{#if book.kobo_removed}
			<p class="kobo-removed">
				<TabletSmartphone size={14} />
				<span>{t('book.koboRemoved')}</span>
				<button type="button" class="ghost" disabled={restoring} onclick={sendToKobo}>
					{t('book.koboRestore')}
				</button>
			</p>
		{/if}

		{#if book.category || book.tags.length > 0 || book.shelves.length > 0}
			<div class="chips">
				{#if book.category}
					<span class="chip category" title={t('book.category')}>
						<Layers size={11} />
						{book.category}
					</span>
				{/if}
				{#each book.tags as tag (tag)}
					<span class="chip tag" title={t('book.tags')}>
						<Tag size={11} />
						{tag}
					</span>
				{/each}
				{#each book.shelves as shelf (shelf.id)}
					<a class="chip shelf" href={`/shelves/${shelf.id}`} title={t('book.shelves')}>
						<BookMarked size={11} />
						{shelf.name}
					</a>
				{/each}
			</div>
		{/if}

		{#if descriptionHtml}
			<div class="description">{@html descriptionHtml}</div>
		{:else}
			<p class="nodesc">{t('book.noDescription')}</p>
		{/if}

		<dl>
			{#if book.language}<dt>{t('book.language')}</dt><dd>{book.language}</dd>{/if}
			{#if book.format !== 'epub'}<dt>{t('book.format')}</dt><dd>{book.format.toUpperCase()}</dd>{/if}
			{#if book.first_published != null}<dt>{t('book.firstPublished')}</dt><dd>{book.first_published}</dd>{/if}
			{#if book.published}<dt>{t('book.edition')}</dt><dd>{formatPublished(book.published, getLocale())}</dd>{/if}
			{#if book.publisher}<dt>{t('book.publisher')}</dt><dd>{book.publisher}</dd>{/if}
			{#if book.isbn}<dt>{t('book.isbn')}</dt><dd>{book.isbn}</dd>{/if}
			{#if book.libris_id}
				<dt>{t('book.libris')}</dt>
				<dd><a href={`https://libris.kb.se/bib/${book.libris_id}`} target="_blank" rel="noreferrer">{book.libris_id}</a></dd>
			{/if}
			{#if book.license}
				<dt>{t('book.license')}</dt>
				<dd>
					{#if licenseUrl(book.license)}
						<a href={licenseUrl(book.license)} target="_blank" rel="noreferrer">{licenseName(book.license)}</a>
					{:else}
						{licenseName(book.license)}
					{/if}
					{#if book.license === 'pd' && book.author_death_year != null}
						({t('book.licenseDied', { year: book.author_death_year })})
					{/if}
					{#if book.license_source_url}
						· <a href={book.license_source_url} target="_blank" rel="noreferrer">{t('book.licenseSource')}</a>
					{/if}
				</dd>
			{/if}
			{#if book.fed_source}
				<dt>{t('book.fedSource')}</dt>
				<dd class="fed-source"><a href={book.fed_source} target="_blank" rel="noreferrer">{fedHost(book.fed_source)}</a></dd>
			{/if}
			<dt>{t('book.fileSize')}</dt><dd>{formatSize(book.file_size)}</dd>
			{#if health}
				<dt>{t('health.heading')}</dt>
				<dd>
					{attention.length === 0
						? t('health.ok')
						: attention.length === 1
							? t('health.remark')
							: t('health.remarks', { count: attention.length })}
					{#if health.fixed.length > 0}
						<span class="fixed">· {t('upload.repaired', { list: fixedList(health) })}</span>
					{/if}
				</dd>
			{/if}
			<dt>{t('book.added')}</dt><dd>{book.created_at.slice(0, 10)}</dd>
		</dl>

		{#if details.length > 0}
			<details class="file-details">
				<summary>{t('health.details')}</summary>
				<p>{t('health.readable')}</p>
				<ul>
					{#each details as issue (issue.code)}
						<li>
							{issueText(issue)}
							{#if issue.examples?.length}
								<span class="examples">{issue.examples.join(' · ')}</span>
							{/if}
						</li>
					{/each}
				</ul>
			</details>
		{/if}
	</div>
</div>

<style>
	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		margin-bottom: 1.25rem;
		color: var(--muted);
	}
	.detail {
		display: grid;
		grid-template-columns: 14rem 1fr;
		gap: 2rem;
		align-items: start;
	}
	@media (max-width: 40rem) {
		.detail {
			grid-template-columns: 1fr;
		}
		.side {
			max-width: 14rem;
		}
	}
	.cover {
		aspect-ratio: 2 / 3;
		border-radius: 3px 8px 8px 3px;
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
		padding: 1rem;
		text-align: center;
		color: var(--muted);
	}
	.progress {
		height: 3px;
		margin-top: 0.6rem;
		border-radius: 2px;
		background: var(--border);
		overflow: hidden;
	}
	.progress span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.read {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.4rem;
		margin-top: 0.75rem;
		padding: 0.5rem 0.8rem;
		border-radius: 4px;
		background: var(--accent);
		color: var(--bg);
		font-size: 0.9rem;
		font-weight: 600;
	}
	.read:hover {
		text-decoration: none;
		filter: brightness(1.08);
	}
	.download {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.4rem;
		margin-top: 0.6rem;
		font-size: 0.9rem;
	}
	.elsewhere {
		margin: 0.7rem 0 0;
		font-size: 0.82rem;
		color: var(--muted);
		text-align: center;
	}
	.kindle-error {
		margin: 0.3rem 0 0;
		font-size: 0.8rem;
		color: var(--danger);
	}
	.want {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.4rem;
		margin-top: 0.45rem;
		width: 100%;
		background: none;
		border: 1px solid var(--border);
		color: var(--fg);
		font-size: 0.85rem;
		font-weight: 500;
	}
	.want:hover {
		border-color: var(--accent);
		filter: none;
	}
	.want.on {
		border-color: var(--accent);
		color: var(--accent);
	}
	.headline {
		display: flex;
		align-items: start;
		justify-content: space-between;
		gap: 1rem;
	}
	h1 {
		margin: 0;
		font-size: 1.5rem;
	}
	.author {
		margin: 0.25rem 0 0;
		color: var(--muted);
	}
	.author a {
		color: inherit;
		text-decoration: none;
		border-bottom: 1px dotted var(--muted);
	}
	.author a:hover {
		color: var(--accent);
		border-bottom-color: var(--accent);
	}
	.edit-btn {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 0.3rem 0.75rem;
		font-size: 0.85rem;
		color: var(--fg);
	}
	.edit-btn:hover {
		text-decoration: none;
		border-color: var(--accent);
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		margin-top: 0.9rem;
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.8rem;
		padding: 0.15rem 0.6rem;
		border-radius: 99px;
		border: 1px solid var(--border);
		color: var(--muted);
	}
	.chip.category {
		border-color: var(--accent);
		color: var(--accent);
	}
	.chip.shelf {
		border-color: var(--gold);
		color: var(--gold);
	}
	a.chip.shelf:hover {
		text-decoration: none;
		background: var(--card);
	}
	.series {
		margin: 0.3rem 0 0;
		font-size: 0.9rem;
	}
	.series a {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		color: var(--gold);
	}
	.kobo-removed {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		margin: 1rem 0 0;
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
		font-size: 0.88rem;
		color: var(--muted);
	}
	.health {
		margin: 1rem 0 0;
		padding: 0.6rem 0.8rem;
		border: 1px solid var(--border);
		border-left: 3px solid var(--gold, var(--accent));
		border-radius: 8px;
		background: var(--card);
		font-size: 0.88rem;
	}
	.health h2 {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		margin: 0 0 0.35rem;
		font-family: inherit;
		font-size: 0.88rem;
		font-weight: 600;
	}
	.health ul {
		margin: 0 0 0.5rem;
		padding-left: 1.1rem;
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
	}
	.health ul:last-child {
		margin-bottom: 0;
	}
	.file-details {
		margin-top: 0.9rem;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.file-details summary {
		cursor: pointer;
	}
	.file-details p {
		margin: 0.4rem 0 0.3rem;
	}
	.file-details ul {
		margin: 0;
		padding-left: 1.1rem;
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
	}
	.examples {
		display: block;
		font-size: 0.78rem;
		overflow-wrap: anywhere;
	}
	.health .error {
		margin: 0.4rem 0 0;
	}
	.fixed {
		color: var(--muted);
	}
	.kobo-removed span {
		flex: 1;
		min-width: 12rem;
	}
	.rating {
		margin: 0.55rem 0 0 -0.15rem;
	}
	.series a:hover {
		color: var(--accent);
		text-decoration: none;
	}
	.description {
		margin-top: 1.25rem;
		max-width: 42rem;
	}
	.description :global(p:first-child) {
		margin-top: 0;
	}
	.nodesc {
		margin-top: 1.25rem;
		color: var(--muted);
		font-style: italic;
	}
	dl {
		margin-top: 1.5rem;
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 0.3rem 1.25rem;
		font-size: 0.9rem;
	}
	dt {
		color: var(--muted);
	}
	dd {
		margin: 0;
	}
</style>
