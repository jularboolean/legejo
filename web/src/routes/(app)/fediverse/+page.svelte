<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { ArrowLeft, Check, Download, UserPlus, X } from '@lucide/svelte';
	import BookPeek from '#lib/BookPeek.svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { fedHost, formatSize } from '#lib/fed';
	import { getLocale, t } from '#lib/i18n';
	import {
		SHARED_SORT_KEYS,
		languageLabel,
		loadSharedSort,
		loadView,
		saveSharedSort,
		saveView,
		sortShared,
		type SortDir,
		type SortKey,
		type ViewMode
	} from '#lib/library';
	import BookGrid from '#lib/library/BookGrid.svelte';
	import BookRows from '#lib/library/BookRows.svelte';
	import SortControl from '#lib/library/SortControl.svelte';
	import ViewSwitch from '#lib/library/ViewSwitch.svelte';
	import { licenseName, reasonText } from '#lib/license';
	import { defaultShelfCover } from '#lib/shelfCovers';
	import type { Book, FedBook, FedFollow, FollowState } from '#lib/types';
	import type { FedConnected } from './+page';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const selected = $derived(data.follows.find((f) => f.actor === data.actor) ?? null);
	/** The book looked at more closely. */
	let peek = $state<FedBook | null>(null);

	const STATE_KEYS: Record<FollowState, Parameters<typeof t>[0]> = {
		pending: 'fed.state.pending',
		accepted: 'fed.state.accepted',
		rejected: 'fed.state.rejected',
		gone: 'fed.state.gone'
	};

	const FOLLOW_ERRORS: Record<string, Parameters<typeof t>[0]> = {
		'federation is off': 'fed.error.off',
		'not found': 'fed.follow.notFound',
		'domain not allowed': 'fed.follow.domainNotAllowed',
		'not a shelf': 'fed.follow.notAShelf',
		'already following': 'fed.follow.already'
	};

	/** What the connected instances list, less what is suggested above it already. */
	function offered(instances: FedConnected[]): FedConnected[] {
		const suggested = new Set(data.suggestions.map((s) => s.handle.toLowerCase()));
		return instances
			.map((i) => ({ ...i, shelves: i.shelves.filter((s) => !suggested.has(s.handle.toLowerCase())) }))
			.filter((i) => i.shelves.length > 0);
	}

	/** Remote summaries may carry HTML; show them as plain text only. */
	function plain(html: string | null): string {
		if (!html) return '';
		return (new DOMParser().parseFromString(html, 'text/html').body.textContent ?? '').trim();
	}

	/** The label under a followed shelf: local approval and reachability come first. */
	function stateKey(f: FedFollow): Parameters<typeof t>[0] {
		if (f.awaiting_approval) return 'fed.state.awaiting';
		if (f.unreachable_since && f.state !== 'gone' && f.state !== 'rejected') return 'fed.state.unreachable';
		return STATE_KEYS[f.state];
	}
	function stateClass(f: FedFollow): string {
		if (f.awaiting_approval) return 'pending';
		if (f.unreachable_since && f.state !== 'gone' && f.state !== 'rejected') return 'unreachable';
		return f.state;
	}

	// Covers are loaded from the remote instance; one that fails falls back
	// to the title placeholder.
	let brokenCovers = $state<Set<string>>(new Set());

	/** A remote book as the lists of the own books take it. */
	type Row = Book & { fed: FedBook };

	function row(book: FedBook, index: number): Row {
		return {
			fed: book,
			id: index + 1,
			uuid: book.object_iri,
			title: book.title,
			author: book.author,
			language: book.language,
			description: book.description ?? book.summary,
			publisher: book.publisher ?? null,
			published: book.edition_date ?? null,
			first_published: book.first_published ?? null,
			category: book.category ?? null,
			identifier: null,
			isbn: book.isbn,
			libris_id: null,
			format: 'epub',
			file_size: book.epub_size ?? 0,
			health_issues: 0,
			has_cover: !!book.cover_url && !brokenCovers.has(book.object_iri),
			created_at: book.published ?? '',
			updated_at: null,
			series: book.series ?? null,
			series_index: book.series_index ? Number(book.series_index) : null,
			rating: null,
			want_to_read: false,
			wanted_at: null,
			license: book.license,
			license_source_url: book.license_source_url,
			author_death_year: book.author_death_year,
			cover_is_free: false
		};
	}

	// As on the own shelves: the same view, and the same remembered order.
	let view = $state<ViewMode>(loadView());
	let order = $state(loadSharedSort());
	const rows = $derived(sortShared((data.books ?? []).map(row), order.sort, order.dir, getLocale()));

	function setView(next: ViewMode) {
		view = next;
		saveView(next);
	}

	function setOrder(sort: SortKey, dir: SortDir) {
		order = { sort, dir };
		saveSharedSort(sort, dir);
	}

	const rowHref = (r: Row) => r.fed.page_url ?? '#';
	const rowCover = (r: Row) => r.fed.cover_url ?? '';
	const breakCover = (r: Row) => (brokenCovers = new Set([...brokenCovers, r.fed.object_iri]));

	/** A stable number for a shelf that has no picture of its own. */
	function coverNumber(actor: string): number {
		let n = 0;
		for (const c of actor) n = (n * 31 + c.charCodeAt(0)) % 9973;
		return n;
	}

	// Opening a shelf makes the server check that its instance still answers.
	// Reload once shortly after, so an unreachable instance shows up.
	$effect(() => {
		if (data.actor == null) return;
		const timer = setTimeout(() => invalidateAll(), 4000);
		return () => clearTimeout(timer);
	});

	function shelfHref(actor: string) {
		return `/fediverse?actor=${encodeURIComponent(actor)}`;
	}

	// ---- Follow ----
	let handleInput = $state('');
	let following = $state(false);
	let followError = $state('');

	async function follow(e: SubmitEvent) {
		e.preventDefault();
		await followHandle(handleInput.trim());
	}

	async function followHandle(handle: string) {
		if (!handle) return;
		following = true;
		followError = '';
		try {
			const res = await fetch('/api/fed/follows', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ handle })
			});
			if (res.ok) {
				const item: FedFollow = await res.json();
				handleInput = '';
				await goto(shelfHref(item.actor), { invalidateAll: true });
			} else {
				const body = await res.json().catch(() => null);
				const key = FOLLOW_ERRORS[body?.error];
				followError = key ? t(key) : (body?.error ?? t('fed.follow.failed'));
			}
		} catch {
			followError = t('common.network');
		} finally {
			following = false;
		}
	}

	// ---- Unfollow ----
	let unfollowTarget = $state<FedFollow | null>(null);
	let listError = $state('');

	async function unfollow() {
		const target = unfollowTarget;
		unfollowTarget = null;
		if (!target) return;
		listError = '';
		try {
			const res = await fetch(`/api/fed/follows?actor=${encodeURIComponent(target.actor)}`, {
				method: 'DELETE'
			});
			if (!res.ok) {
				listError = t('fed.unfollowFailed');
				return;
			}
			if (data.actor === target.actor) {
				await goto('/fediverse', { invalidateAll: true });
			} else {
				await invalidateAll();
			}
		} catch {
			listError = t('common.network');
		}
	}

	// ---- Import ----
	let importing = $state<Set<string>>(new Set());
	// object_iri → id of the new local book, for books fetched on this visit.
	let imported = $state<Record<string, number>>({});
	let importErrors = $state<Record<string, string>>({});

	async function importBook(iri: string) {
		importing = new Set([...importing, iri]);
		const rest = { ...importErrors };
		delete rest[iri];
		importErrors = rest;
		let msg = '';
		try {
			const res = await fetch('/api/fed/import', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ object_iri: iri })
			});
			const body = await res.json().catch(() => null);
			if (res.ok && body?.id != null) {
				imported = { ...imported, [iri]: body.id };
				// The new book changes the library and shelf counts.
				await invalidateAll();
			} else if (res.status === 409 && body?.book_id != null) {
				imported = { ...imported, [iri]: body.book_id };
			} else if (res.status === 502) {
				msg = t('fed.import.fetchFailed');
			} else if (body?.reason) {
				msg = reasonText(body.reason);
			} else {
				msg = body?.error ?? t('fed.import.failed');
			}
		} catch {
			msg = t('common.network');
		} finally {
			const next = new Set(importing);
			next.delete(iri);
			importing = next;
		}
		if (msg) importErrors = { ...importErrors, [iri]: msg };
	}
</script>

{#if data.actor == null}<h1>{t('fed.heading')}</h1>{/if}

{#if !data.fed.available}
	<p class="muted">{t('fed.unavailable')}</p>
{:else if !data.fed.enabled}
	<p class="muted">{t('fed.turnedOff')} <a href="/account?tab=options">{t('nav.account')}</a></p>
{:else}
	<div class="layout" class:has-selection={data.actor != null}>
		<section class="follows">
			<form class="follow-form" onsubmit={follow}>
				<label for="follow-handle">{t('fed.follow.label')}</label>
				<div class="follow-row">
					<input
						id="follow-handle"
						bind:value={handleInput}
						placeholder={t('fed.follow.placeholder')}
						autocapitalize="off"
						autocomplete="off"
						spellcheck="false"
					/>
					<button type="submit" disabled={following || !handleInput.trim()}>
						<UserPlus size={13} />
						{following ? t('fed.follow.following') : t('fed.follow.submit')}
					</button>
				</div>
				<p class="hint">{t('fed.follow.hint')}</p>
				{#if followError}<p class="error">{followError}</p>{/if}
			</form>

			{#if data.suggestions.length > 0}
				<h2>{t('fed.suggested')}</h2>
				<ul class="suggestions">
					{#each data.suggestions as s (s.handle)}
						<li lang={s.language}>
							<strong>{s.name}</strong>
							<span class="follow-handle">{s.handle}</span>
							<p class="hint">{s.summary}</p>
							<button type="button" onclick={() => followHandle(s.handle)} disabled={following}>
								<UserPlus size={13} />
								{t('fed.follow.submit')}
							</button>
						</li>
					{/each}
				</ul>
			{/if}

			{#await data.connected.then(offered) then instances}
				{#if instances.length > 0}
					<h2>{t('fed.connected')}</h2>
					{#each instances as instance (instance.domain)}
						<h3 class="instance">{instance.domain}</h3>
						<ul class="suggestions">
							{#each instance.shelves as s (s.handle)}
								<li>
									<strong>{s.name}</strong>
									<span class="follow-handle">{s.handle}</span>
									{#if s.summary}<p class="hint">{s.summary}</p>{/if}
									<span class="follow-handle">{s.books === 1 ? t('shelf.book') : t('shelf.books', { count: s.books })}</span>
									<button type="button" onclick={() => followHandle(s.handle)} disabled={following}>
										<UserPlus size={13} />
										{t('fed.follow.submit')}
									</button>
								</li>
							{/each}
						</ul>
					{/each}
				{/if}
			{/await}

			<h2>{t('fed.followed')}</h2>
			{#if listError}<p class="error">{listError}</p>{/if}
			{#if data.follows.length === 0}
				<p class="muted">{t('fed.noFollows')}</p>
			{:else}
				<ul class="follow-list">
					{#each data.follows as f (f.actor)}
						<li class:active={f.actor === data.actor}>
							<a class="follow-link" href={shelfHref(f.actor)}>
								<span class="follow-name">{f.name}</span>
								<span class="follow-handle">{f.handle}</span>
								<span class="follow-meta">
									<span class="state state-{stateClass(f)}">{t(stateKey(f))}</span>
									<span>
										{f.book_count === 1
											? t('shelf.book')
											: t('shelf.books', { count: f.book_count })}
									</span>
								</span>
							</a>
							<button
								type="button"
								class="unfollow"
								title={t('fed.unfollow')}
								aria-label={t('fed.unfollowNamed', { name: f.name })}
								onclick={() => (unfollowTarget = f)}
							>
								<X size={14} />
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<section class="books">
			{#if data.actor == null}
				<p class="muted pick">{t('fed.pickShelf')}</p>
			{:else}
				<a class="back" href="/fediverse"><ArrowLeft size={13} /> {t('fed.backToList')}</a>
				{#if selected}
					<header class="hero">
						<img class="hero-image" src={defaultShelfCover(coverNumber(selected.actor))} alt="" />
						<div class="hero-shade"></div>
						<div class="hero-content">
							<h1>{selected.name}</h1>
							<p class="meta">
								{data.books
									? data.books.length === 1
										? t('shelf.book')
										: t('shelf.books', { count: data.books.length })
									: ''}
								<span class="handle">{selected.handle}</span>
							</p>
						</div>
					</header>
					{#if selected.awaiting_approval}
						<p class="hint state-note">{t('fed.awaitingHint', { domain: fedHost(selected.actor) })}</p>
					{:else if selected.unreachable_since && selected.state !== 'gone' && selected.state !== 'rejected'}
						<p class="hint warn state-note">
							{t('fed.unreachableHint', {
								domain: fedHost(selected.actor),
								date: selected.unreachable_since.slice(0, 10)
							})}
						</p>
					{:else if selected.state === 'pending'}
						<p class="hint state-note">{t('fed.pendingHint')}</p>
					{:else if selected.state === 'rejected'}
						<p class="hint state-note">{t('fed.rejectedHint')}</p>
					{:else if selected.state === 'gone'}
						<p class="hint state-note">{t('fed.goneHint')}</p>
					{/if}
				{/if}

				<div class="intro">
					{#if selected && plain(selected.summary)}
						<p class="summary">{plain(selected.summary)}</p>
					{/if}
					{#if rows.length > 0}
						<div class="switch">
							{#if rows.length > 1}
								<SortControl keys={SHARED_SORT_KEYS} sort={order.sort} dir={order.dir} onchange={setOrder} />
							{/if}
							<ViewSwitch {view} onchange={setView} />
						</div>
					{/if}
				</div>

				{#snippet facts(r: Row)}
					{@const book = r.fed}
					<span class="facts">
						{#if book.license}<span>{licenseName(book.license)}</span>{/if}
						{#if book.epub_size != null}<span>{formatSize(book.epub_size)}</span>{/if}
						{#if book.license_source_url}
							<a href={book.license_source_url} target="_blank" rel="noreferrer">
								{fedHost(book.license_source_url)}
							</a>
						{/if}
					</span>
					{#if importErrors[book.object_iri]}
						<span class="error small">{importErrors[book.object_iri]}</span>
					{/if}
				{/snippet}

				{#snippet importAction(r: Row)}
					{@const book = r.fed}
					{@const localId = imported[book.object_iri] ?? book.imported_book_id}
					{#if localId != null}
						<a class="done" href={`/books/${localId}`}>
							<Check size={13} />
							{imported[book.object_iri] != null ? t('fed.import.done') : t('fed.import.owned')}
						</a>
					{:else}
						<button
							type="button"
							class="ghost import"
							disabled={importing.has(book.object_iri)}
							onclick={() => importBook(book.object_iri)}
						>
							<Download size={13} />
							{importing.has(book.object_iri) ? t('fed.import.importing') : t('fed.import.button')}
						</button>
					{/if}
				{/snippet}

				{#if data.booksFailed}
					<p class="error">{t('fed.booksFailed')}</p>
				{:else if rows.length === 0}
					<p class="muted">{t('fed.noBooks')}</p>
				{:else}
					<p class="hint import-hint">{t('fed.import.hint')}</p>
					{#if view === 'list'}
						<BookRows
							books={rows}
							href={rowHref}
							cover={rowCover}
							onopen={(r) => (peek = r.fed)}
							oncoverfail={breakCover}
							own={false}
							showLanguage
							context={facts}
							action={importAction}
						/>
					{:else}
						<BookGrid
							books={rows}
							href={rowHref}
							coverSrc={rowCover}
							onopen={(r) => (peek = r.fed)}
							oncoverfail={breakCover}
							action={importAction}
						/>
					{/if}
				{/if}
			{/if}
		</section>
	</div>
{/if}

{#if peek}
	{@const book = peek}
	{@const localId = imported[book.object_iri] ?? book.imported_book_id}
	<BookPeek
		open
		title={book.title}
		author={book.author}
		cover={book.cover_url}
		description={book.description ?? book.summary}
		facts={[
			{ label: t('book.series'), value: book.series ? `${book.series}${book.series_index ? ` #${book.series_index}` : ''}` : null },
			{ label: t('book.firstPublished'), value: book.first_published != null ? String(book.first_published) : null },
			{ label: t('book.published'), value: book.edition_date },
			{ label: t('book.publisher'), value: book.publisher },
			{ label: t('book.language'), value: book.language ? languageLabel(book.language, getLocale()) : null },
			{ label: t('edit.category'), value: book.category },
			{ label: t('edit.isbn'), value: book.isbn },
			{ label: t('book.license'), value: book.license ? licenseName(book.license) : null },
			{ label: t('book.format'), value: book.epub_size != null ? `EPUB · ${formatSize(book.epub_size)}` : 'EPUB' }
		]}
		link={book.page_url ? { href: book.page_url, label: t('bookPeek.onInstance', { host: fedHost(book.page_url) }) } : null}
		onclose={() => (peek = null)}
	>
		{#snippet actions()}
			{#if localId != null}
				<a class="done" href={`/books/${localId}`}>
					<Check size={13} />
					{imported[book.object_iri] != null ? t('fed.import.done') : t('fed.import.owned')}
				</a>
			{:else}
				<button type="button" disabled={importing.has(book.object_iri)} onclick={() => importBook(book.object_iri)}>
					<Download size={13} />
					{importing.has(book.object_iri) ? t('fed.import.importing') : t('fed.import.button')}
				</button>
			{/if}
			{#if book.license_source_url}
				<a href={book.license_source_url} target="_blank" rel="noreferrer">{fedHost(book.license_source_url)} ↗</a>
			{/if}
		{/snippet}
	</BookPeek>
{/if}

<ConfirmDialog
	open={unfollowTarget != null}
	title={t('fed.unfollow')}
	message={unfollowTarget ? t('fed.unfollowConfirm', { name: unfollowTarget.name }) : ''}
	confirmLabel={t('fed.unfollow')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={unfollow}
	oncancel={() => (unfollowTarget = null)}
/>

<style>
	h1 {
		font-size: 1.4rem;
		margin: 0 0 1.5rem;
	}
	h2 {
		font-size: 1.1rem;
		margin: 0 0 0.6rem;
	}
	.muted {
		color: var(--muted);
		font-size: 0.9rem;
	}
	.hint {
		margin: 0.35rem 0 0;
		font-size: 0.8rem;
		color: var(--muted);
	}
	.error.small {
		font-size: 0.8rem;
	}
	.layout {
		display: grid;
		grid-template-columns: minmax(15rem, 20rem) minmax(0, 1fr);
		gap: 2rem;
		align-items: start;
	}
	.follow-form {
		margin-bottom: 1.75rem;
	}
	.follow-form label {
		display: block;
		font-size: 0.9rem;
		color: var(--muted);
		margin-bottom: 0.3rem;
	}
	.follow-row {
		display: flex;
		gap: 0.5rem;
	}
	.follow-row input {
		min-width: 0;
	}
	.follow-row button {
		flex-shrink: 0;
	}
	.follow-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.follow-list li {
		display: flex;
		align-items: flex-start;
		gap: 0.25rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
	}
	.follow-list li.active {
		border-color: var(--accent);
	}
	.follow-link {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		padding: 0.55rem 0.75rem;
		color: var(--fg);
	}
	.follow-link:hover {
		text-decoration: none;
	}
	.follow-link:hover .follow-name {
		color: var(--accent);
	}
	.follow-name {
		font-weight: 600;
		font-size: 0.95rem;
	}
	.suggestions {
		list-style: none;
		padding: 0;
		margin: 0 0 1.5rem;
	}
	.suggestions li {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.25rem;
		padding: 0.75rem;
		border: 1px solid var(--border);
		border-radius: 10px;
	}
	.instance {
		font-size: 0.95rem;
		margin: 0 0 0.5rem;
	}
	.suggestions li + li {
		margin-top: 0.5rem;
	}
	.suggestions .hint {
		margin: 0;
	}
	.follow-handle {
		font-family: ui-monospace, 'SF Mono', monospace;
		font-size: 0.78rem;
		color: var(--muted);
		overflow-wrap: anywhere;
		margin: 0;
	}
	.follow-meta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		margin-top: 0.3rem;
		font-size: 0.78rem;
		color: var(--muted);
	}
	.state {
		border: 1px solid var(--border);
		border-radius: 99px;
		padding: 0 0.5rem;
	}
	.state-accepted {
		color: var(--accent);
		border-color: var(--accent);
	}
	.state-unreachable,
	.state-rejected,
	.state-gone {
		color: var(--danger);
		border-color: var(--danger);
	}
	.unfollow {
		background: none;
		border: none;
		color: var(--muted);
		padding: 0.5rem;
		border-radius: 6px;
		margin: 0.2rem;
	}
	.unfollow:hover {
		filter: none;
		color: var(--danger);
		background: var(--bg);
	}
	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		margin-bottom: 1rem;
		color: var(--muted);
	}
	.pick {
		margin-top: 2.2rem;
	}
	.hero {
		position: relative;
		display: flex;
		align-items: flex-end;
		min-height: 9rem;
		border-radius: 12px;
		overflow: hidden;
		margin-bottom: 1.5rem;
	}
	.hero-image {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.hero-shade {
		position: absolute;
		inset: 0;
		background: linear-gradient(to top, rgba(0, 0, 0, 0.65) 0%, rgba(0, 0, 0, 0.1) 55%, transparent);
	}
	.hero-content {
		position: relative;
		width: 100%;
		min-width: 0;
		padding: 0.9rem 1.25rem;
		color: #fff;
	}
	.hero h1 {
		margin: 0;
		font-size: 1.8rem;
		overflow-wrap: anywhere;
		text-shadow: 0 1px 4px rgba(0, 0, 0, 0.4);
	}
	.meta {
		margin: 0.25rem 0 0;
		color: rgba(255, 255, 255, 0.85);
		font-size: 0.9rem;
	}
	.handle {
		margin-left: 0.35rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		font-size: 0.8rem;
		overflow-wrap: anywhere;
	}
	.state-note {
		margin: -0.75rem 0 1.25rem;
	}
	/* On a narrow screen the sort and view buttons wrap below the description. */
	.intro {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-start;
		gap: 1rem 1.5rem;
		margin: 0.25rem 0 1.25rem;
	}
	.switch {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.summary {
		flex: 1 1 20rem;
		min-width: 0;
		max-width: 42rem;
		margin: 0;
		padding-left: 1rem;
		border-left: 3px solid var(--gold);
		color: var(--muted);
		font-size: 0.95rem;
		line-height: 1.6;
		overflow-wrap: anywhere;
	}
	.import-hint {
		margin: 0 0 0.75rem;
	}
	.facts {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 0.2rem 0.75rem;
		color: var(--muted);
	}
	.done {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.3rem;
		padding: 0.3rem 0;
		font-size: 0.8rem;
		white-space: nowrap;
	}
	.import {
		justify-content: center;
	}
	.import :global(svg) {
		flex-shrink: 0;
	}
	/* A shelf has the page to itself, as an own shelf has. */
	.layout.has-selection {
		grid-template-columns: minmax(0, 1fr);
	}
	.layout.has-selection .follows {
		display: none;
	}
	@media (max-width: 60rem) {
		.layout {
			grid-template-columns: minmax(0, 1fr);
			gap: 1.5rem;
		}
		.layout:not(.has-selection) .books {
			display: none;
		}
	}
	.hint.warn {
		color: var(--danger);
	}
</style>
