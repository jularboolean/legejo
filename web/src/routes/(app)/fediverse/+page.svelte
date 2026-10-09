<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { ArrowLeft, Check, Download, UserPlus, X } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { fedHost, formatSize } from '#lib/fed';
	import { t } from '#lib/i18n';
	import { licenseName, reasonText } from '#lib/license';
	import type { FedFollow, FollowState } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const selected = $derived(data.follows.find((f) => f.actor === data.actor) ?? null);

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
		const handle = handleInput.trim();
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

<h1>{t('fed.heading')}</h1>

{#if !data.fed.available}
	<p class="muted">{t('fed.unavailable')}</p>
{:else if !data.fed.enabled}
	<p class="muted">{t('fed.turnedOff')} <a href="/account">{t('nav.account')}</a></p>
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
					<header class="shelf-head">
						<h2>{selected.name}</h2>
						<p class="follow-handle">{selected.handle}</p>
						{#if plain(selected.summary)}
							<p class="summary">{plain(selected.summary)}</p>
						{/if}
						{#if selected.awaiting_approval}
							<p class="hint">{t('fed.awaitingHint', { domain: fedHost(selected.actor) })}</p>
						{:else if selected.unreachable_since && selected.state !== 'gone' && selected.state !== 'rejected'}
							<p class="hint warn">
								{t('fed.unreachableHint', {
									domain: fedHost(selected.actor),
									date: selected.unreachable_since.slice(0, 10)
								})}
							</p>
						{:else if selected.state === 'pending'}
							<p class="hint">{t('fed.pendingHint')}</p>
						{:else if selected.state === 'rejected'}
							<p class="hint">{t('fed.rejectedHint')}</p>
						{:else if selected.state === 'gone'}
							<p class="hint">{t('fed.goneHint')}</p>
						{/if}
					</header>
				{/if}

				{#if data.booksFailed}
					<p class="error">{t('fed.booksFailed')}</p>
				{:else if !data.books || data.books.length === 0}
					<p class="muted">{t('fed.noBooks')}</p>
				{:else}
					<p class="hint">{t('fed.import.hint')}</p>
					<ul class="book-list">
						{#each data.books as book (book.object_iri)}
							{@const localId = imported[book.object_iri] ?? book.imported_book_id}
							<li class="book">
								<div class="cover">
									{#if book.cover_url && !brokenCovers.has(book.object_iri)}
										<img
											src={book.cover_url}
											alt=""
											loading="lazy"
											referrerpolicy="no-referrer"
											onerror={() => (brokenCovers = new Set([...brokenCovers, book.object_iri]))}
										/>
									{:else}
										<span class="placeholder">{book.title}</span>
									{/if}
								</div>
								<div class="info">
									<span class="title">{book.title}</span>
									{#if book.author}<span class="author">{book.author}</span>{/if}
									<span class="facts">
										{#if book.license}<span>{licenseName(book.license)}</span>{/if}
										{#if book.published}<span>{book.published.slice(0, 4)}</span>{/if}
										{#if book.language}<span>{book.language}</span>{/if}
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
								</div>
								<div class="action">
									{#if localId != null}
										<a class="done" href={`/books/${localId}`}>
											<Check size={13} />
											{imported[book.object_iri] != null ? t('fed.import.done') : t('fed.import.owned')}
										</a>
									{:else}
										<button
											type="button"
											class="ghost"
											disabled={importing.has(book.object_iri)}
											onclick={() => importBook(book.object_iri)}
										>
											<Download size={13} />
											{importing.has(book.object_iri) ? t('fed.import.importing') : t('fed.import.button')}
										</button>
									{/if}
								</div>
							</li>
						{/each}
					</ul>
				{/if}
			{/if}
		</section>
	</div>
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
		display: none;
		align-items: center;
		gap: 0.3rem;
		margin-bottom: 1rem;
		color: var(--muted);
	}
	.pick {
		margin-top: 2.2rem;
	}
	.shelf-head {
		margin-bottom: 1rem;
	}
	.shelf-head h2 {
		margin: 0;
		font-size: 1.3rem;
	}
	.summary {
		margin: 0.6rem 0 0;
		padding-left: 1rem;
		border-left: 3px solid var(--gold);
		color: var(--muted);
		font-size: 0.92rem;
		max-width: 42rem;
	}
	.book-list {
		list-style: none;
		margin: 1rem 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
	}
	.book {
		display: grid;
		grid-template-columns: 3.4rem minmax(0, 1fr) auto;
		gap: 0.9rem;
		align-items: center;
		padding: 0.7rem 0;
		border-top: 1px solid var(--border);
	}
	.book:last-child {
		border-bottom: 1px solid var(--border);
	}
	.cover {
		aspect-ratio: 2 / 3;
		border-radius: 2px 4px 4px 2px;
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
		padding: 0.2rem;
		text-align: center;
		font-size: 0.55rem;
		line-height: 1.2;
		color: var(--muted);
		overflow: hidden;
	}
	.info {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.title {
		font-weight: 600;
		font-size: 0.95rem;
	}
	.author {
		font-size: 0.85rem;
		color: var(--muted);
	}
	.facts {
		display: flex;
		flex-wrap: wrap;
		gap: 0.2rem 0.75rem;
		margin-top: 0.2rem;
		font-size: 0.78rem;
		color: var(--muted);
	}
	.done {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.8rem;
		white-space: nowrap;
	}
	.action button {
		white-space: nowrap;
	}
	@media (max-width: 60rem) {
		.layout {
			grid-template-columns: minmax(0, 1fr);
			gap: 1.5rem;
		}
		/* One pane at a time on narrow screens: the list, or one shelf. */
		.layout.has-selection .follows {
			display: none;
		}
		.layout:not(.has-selection) .books {
			display: none;
		}
		.back {
			display: inline-flex;
		}
	}
	@media (max-width: 30rem) {
		.book {
			grid-template-columns: 3rem minmax(0, 1fr);
		}
		.action {
			grid-column: 2;
		}
	}
	.hint.warn {
		color: var(--danger);
	}
</style>
