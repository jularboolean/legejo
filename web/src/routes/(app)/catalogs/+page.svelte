<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { navigating } from '$app/state';
	import { ArrowLeft, Check, ChevronRight, Download, Plus, Search, X } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { fedHost, formatSize } from '#lib/fed';
	import { t } from '#lib/i18n';
	import type { Catalog, CatalogEntry, CatalogFile } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const selected = $derived(data.catalogs.find((c) => c.id === data.selected) ?? null);

	/** Catalogs known to work, offered until the user has them. */
	const SUGGESTED: { title: string; url: string; about: Parameters<typeof t>[0] }[] = [
		{ title: 'Project Gutenberg', url: 'https://www.gutenberg.org/ebooks.opds/', about: 'catalogs.suggest.gutenberg' },
		{ title: 'Unglue.it', url: 'https://unglue.it/api/opds/', about: 'catalogs.suggest.unglue' },
		{
			title: 'Ebooks libres et gratuits',
			url: 'https://www.ebooksgratuits.com/opds/index.php',
			about: 'catalogs.suggest.ebooksgratuits'
		},
		{ title: 'Bibliothèque numérique romande', url: 'https://ebooks-bnr.com/opds/', about: 'catalogs.suggest.bnr' },
		{ title: 'Izzy’s freie Bibliothek', url: 'https://ebooks.qumran.org/opds/?lang=de', about: 'catalogs.suggest.qumran' },
		{ title: 'textos.info', url: 'https://www.textos.info/catalogo.atom', about: 'catalogs.suggest.textos' }
	];
	const suggestions = $derived(SUGGESTED.filter((s) => !data.catalogs.some((c) => c.url === s.url)));

	function catalogHref(id: number, feed?: string | null, q?: string) {
		const params = new URLSearchParams({ c: String(id) });
		if (feed) params.set('url', feed);
		if (q) params.set('q', q);
		return `/catalogs?${params}`;
	}

	// A page of a catalog is fetched from the catalog on the way here, which
	// takes a moment: the pane is dimmed meanwhile.
	const loading = $derived(navigating.to?.url.pathname === '/catalogs');

	// ---- Add ----
	const ADD_ERRORS: Record<string, Parameters<typeof t>[0]> = {
		'not a catalog': 'catalogs.add.notCatalog',
		'address not allowed': 'catalogs.add.notAllowed',
		'fetch failed': 'catalogs.add.unreachable',
		refused: 'catalogs.add.refused',
		missing: 'catalogs.add.notCatalog',
		busy: 'catalogs.feedBusy',
		'too large': 'catalogs.add.notCatalog',
		'already added': 'catalogs.add.already',
		'too many catalogs': 'catalogs.add.tooMany'
	};
	let addressInput = $state('');
	let adding = $state('');
	let addError = $state('');

	async function add(url: string) {
		if (!url || adding) return;
		adding = url;
		addError = '';
		try {
			const res = await fetch('/api/catalogs', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ url })
			});
			const body = await res.json().catch(() => null);
			if (res.ok && body?.id != null) {
				addressInput = '';
				await goto(catalogHref(body.id), { invalidateAll: true });
			} else if (res.status === 409 && body?.id != null) {
				await goto(catalogHref(body.id));
			} else {
				const key = ADD_ERRORS[body?.error];
				addError = key ? t(key) : t('catalogs.add.failed');
			}
		} catch {
			addError = t('common.network');
		} finally {
			adding = '';
		}
	}

	function submitAdd(e: SubmitEvent) {
		e.preventDefault();
		add(addressInput.trim());
	}

	// ---- Remove ----
	let removeTarget = $state<Catalog | null>(null);
	let listError = $state('');

	async function remove() {
		const target = removeTarget;
		removeTarget = null;
		if (!target) return;
		listError = '';
		try {
			const res = await fetch(`/api/catalogs/${target.id}`, { method: 'DELETE' });
			if (!res.ok) {
				listError = t('catalogs.removeFailed');
			} else if (data.selected === target.id) {
				await goto('/catalogs', { invalidateAll: true });
			} else {
				await invalidateAll();
			}
		} catch {
			listError = t('common.network');
		}
	}

	// ---- Search ----
	let terms = $state('');
	$effect(() => {
		terms = data.q;
	});

	function search(e: SubmitEvent) {
		e.preventDefault();
		if (!selected || !terms.trim()) return;
		goto(catalogHref(selected.id, null, terms.trim()));
	}

	// ---- Fetch ----
	const FETCH_ERRORS: Record<string, Parameters<typeof t>[0]> = {
		'copy-protected': 'catalogs.protected',
		'address not allowed': 'catalogs.add.notAllowed'
	};
	let fetching = $state<Set<string>>(new Set());
	// File address → id of the book it became, for books fetched on this visit.
	let fetched = $state<Record<string, { id: number; had: boolean }>>({});
	let fetchErrors = $state<Record<string, string>>({});
	let brokenCovers = $state<Set<string>>(new Set());

	async function fetchBook(entry: CatalogEntry, file: CatalogFile) {
		if (!selected) return;
		fetching = new Set([...fetching, file.href]);
		const rest = { ...fetchErrors };
		delete rest[file.href];
		fetchErrors = rest;
		let msg = '';
		try {
			const res = await fetch(`/api/catalogs/${selected.id}/import`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ href: file.href, title: entry.title, format: file.format })
			});
			const body = await res.json().catch(() => null);
			if (res.ok && body?.id != null) {
				fetched = { ...fetched, [file.href]: { id: body.id, had: false } };
			} else if (res.status === 409 && body?.book_id != null) {
				fetched = { ...fetched, [file.href]: { id: body.book_id, had: true } };
			} else if (res.status === 502) {
				msg = t('catalogs.fetchFailed');
			} else {
				const key = FETCH_ERRORS[body?.error];
				msg = key ? t(key) : t('catalogs.notABook');
			}
		} catch {
			msg = t('common.network');
		} finally {
			const next = new Set(fetching);
			next.delete(file.href);
			fetching = next;
		}
		if (msg) fetchErrors = { ...fetchErrors, [file.href]: msg };
	}

	function coverSrc(id: number, cover: string) {
		return `/api/catalogs/${id}/image?url=${encodeURIComponent(cover)}`;
	}

	/** "EPUB3 (with images) · 1.2 MB", or just the format when the catalog says no more. */
	function fileLabel(file: CatalogFile) {
		const name = file.title ?? file.format.toUpperCase();
		return file.size ? `${name} · ${formatSize(file.size)}` : name;
	}
</script>

<h1>{t('catalogs.heading')}</h1>

{#if !data.catalogsAvailable}
	<p class="muted">{t('catalogs.unavailable')}</p>
{:else if !data.available}
	<p class="muted">{t('catalogs.turnedOff')} <a href="/account?tab=options">{t('nav.account')}</a></p>
{:else}
<div class="layout" class:has-selection={data.selected != null}>
	<section class="mine">
		<p class="intro">{t('catalogs.intro')}</p>
		<form class="add-form" onsubmit={submitAdd}>
			<label for="catalog-address">{t('catalogs.add.label')}</label>
			<div class="add-row">
				<input
					id="catalog-address"
					bind:value={addressInput}
					placeholder="https://"
					inputmode="url"
					autocapitalize="off"
					autocomplete="off"
					spellcheck="false"
				/>
				<button type="submit" disabled={!!adding || !addressInput.trim()}>
					<Plus size={13} />
					{adding && adding === addressInput.trim() ? t('catalogs.add.adding') : t('catalogs.add.submit')}
				</button>
			</div>
			<p class="hint">{t('catalogs.add.hint')}</p>
			{#if addError}<p class="error">{addError}</p>{/if}
		</form>

		<h2>{t('catalogs.mine')}</h2>
		{#if listError}<p class="error">{listError}</p>{/if}
		{#if data.catalogs.length === 0}
			<p class="muted">{t('catalogs.none')}</p>
		{:else}
			<ul class="catalog-list">
				{#each data.catalogs as c (c.id)}
					<li class:active={c.id === data.selected}>
						<a class="catalog-link" href={catalogHref(c.id)}>
							<span class="catalog-name">{c.title}</span>
							<span class="catalog-host">{fedHost(c.url)}</span>
						</a>
						<button
							type="button"
							class="remove"
							title={t('catalogs.remove')}
							aria-label={t('catalogs.removeNamed', { name: c.title })}
							onclick={() => (removeTarget = c)}
						>
							<X size={14} />
						</button>
					</li>
				{/each}
			</ul>
		{/if}

		{#if suggestions.length > 0}
			<h2 class="suggested-head">{t('catalogs.suggested')}</h2>
			<ul class="catalog-list">
				{#each suggestions as s (s.url)}
					<li class="suggestion">
						<span class="catalog-link">
							<span class="catalog-name">{s.title}</span>
							<span class="catalog-about">{t(s.about)}</span>
						</span>
						<button type="button" class="ghost" disabled={!!adding} onclick={() => add(s.url)}>
							<Plus size={13} />
							{adding === s.url ? t('catalogs.add.adding') : t('catalogs.add.submit')}
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<section class="feed" class:loading aria-busy={loading}>
		{#if data.selected == null || !selected}
			<p class="muted pick">{t('catalogs.pick')}</p>
		{:else}
			<a class="back" href="/catalogs"><ArrowLeft size={13} /> {t('catalogs.back')}</a>
			<header class="feed-head">
				<h2>{selected.title}</h2>
				{#if !data.atStart}
					<a class="start" href={catalogHref(selected.id)}>{t('catalogs.start')}</a>
				{/if}
			</header>
			{#if selected.searchable}
				<form class="search-form" onsubmit={search} role="search">
					<input
						bind:value={terms}
						type="search"
						aria-label={t('catalogs.search.label', { name: selected.title })}
						placeholder={t('catalogs.search.label', { name: selected.title })}
					/>
					<button type="submit" class="ghost" disabled={!terms.trim()}>
						<Search size={13} />
						{t('catalogs.search.submit')}
					</button>
				</form>
			{/if}

			{#if data.pageError || !data.page}
				<p class="error">
					{data.pageError === 'refused'
						? t('catalogs.feedRefused')
						: data.pageError === 'missing'
							? t('catalogs.feedMissing')
							: data.pageError === 'busy'
								? t('catalogs.feedBusy')
								: t('catalogs.feedFailed')}
				</p>
			{:else}
				{#if data.page.title && data.page.title !== selected.title}
					<h3>{data.page.title}</h3>
				{/if}
				{#if data.page.entries.length === 0}
					<p class="muted">{t('catalogs.empty')}</p>
				{:else}
					{#if data.page.entries.some((e) => e.files.length > 0)}
						<p class="hint rights">{t('catalogs.rightsHint')}</p>
					{/if}
					<ul class="entries">
						{#each data.page.entries as entry, i (i)}
							{#if entry.files.length === 0 && entry.href}
								<li class="section">
									<a href={catalogHref(selected.id, entry.href)}>
										<span class="info">
											<span class="title">{entry.title}</span>
											{#if entry.authors.length}<span class="author">{entry.authors.join(', ')}</span>{/if}
											{#if entry.summary}<span class="about">{entry.summary}</span>{/if}
										</span>
										<ChevronRight size={16} />
									</a>
								</li>
							{:else}
								<li class="book">
									<div class="cover">
										{#if entry.cover && !brokenCovers.has(entry.cover)}
											<img
												src={coverSrc(selected.id, entry.cover)}
												alt=""
												loading="lazy"
												onerror={() => (brokenCovers = new Set([...brokenCovers, entry.cover ?? '']))}
											/>
										{:else}
											<span class="placeholder">{entry.title}</span>
										{/if}
									</div>
									<div class="info">
										<span class="title">{entry.title}</span>
										{#if entry.authors.length}<span class="author">{entry.authors.join('; ')}</span>{/if}
										<span class="facts">
											{#if entry.language}<span>{entry.language}</span>{/if}
											{#if entry.rights}<span>{entry.rights}</span>{/if}
										</span>
										{#if entry.summary}<p class="summary">{entry.summary}</p>{/if}
										<div class="files">
											{#each entry.files as file (file.href)}
												{@const got = fetched[file.href]}
												{#if got}
													<a class="done" href={`/books/${got.id}`}>
														<Check size={13} />
														{got.had ? t('fed.import.owned') : t('fed.import.done')}
													</a>
												{:else}
													<button
														type="button"
														class="ghost"
														disabled={fetching.has(file.href)}
														title={t('fed.import.button')}
														onclick={() => fetchBook(entry, file)}
													>
														<Download size={13} />
														{fetching.has(file.href)
															? t('fed.import.importing')
															: entry.files.length > 1
																? fileLabel(file)
																: t('fed.import.button')}
													</button>
												{/if}
												{#if fetchErrors[file.href]}
													<span class="error small">{fetchErrors[file.href]}</span>
												{/if}
											{/each}
											{#if entry.files.length === 0}
												<span class="muted small">{t('catalogs.noFile')}</span>
											{/if}
										</div>
									</div>
								</li>
							{/if}
						{/each}
					</ul>
					{#if data.page.previous || data.page.next}
						<nav class="pager">
							{#if data.page.previous}
								<a href={catalogHref(selected.id, data.page.previous, data.q)}>← {t('catalogs.previous')}</a>
							{:else}
								<span></span>
							{/if}
							{#if data.page.next}
								<a href={catalogHref(selected.id, data.page.next, data.q)}>{t('catalogs.next')} →</a>
							{/if}
						</nav>
					{/if}
				{/if}
			{/if}
		{/if}
	</section>
</div>
{/if}

<ConfirmDialog
	open={removeTarget != null}
	title={t('catalogs.remove')}
	message={removeTarget ? t('catalogs.removeConfirm', { name: removeTarget.title }) : ''}
	confirmLabel={t('catalogs.remove')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={remove}
	oncancel={() => (removeTarget = null)}
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
	h3 {
		font-size: 1rem;
		margin: 1.2rem 0 0;
	}
	.muted {
		color: var(--muted);
		font-size: 0.9rem;
	}
	.small {
		font-size: 0.8rem;
	}
	.hint {
		margin: 0.35rem 0 0;
		font-size: 0.8rem;
		color: var(--muted);
	}
	.intro {
		margin: 0 0 1.25rem;
		font-size: 0.9rem;
		color: var(--muted);
	}
	.layout {
		display: grid;
		grid-template-columns: minmax(15rem, 20rem) minmax(0, 1fr);
		gap: 2rem;
		align-items: start;
	}
	.add-form {
		margin-bottom: 1.75rem;
	}
	.add-form label {
		display: block;
		font-size: 0.9rem;
		color: var(--muted);
		margin-bottom: 0.3rem;
	}
	.add-row,
	.search-form {
		display: flex;
		gap: 0.5rem;
	}
	.add-row input,
	.search-form input {
		min-width: 0;
		flex: 1;
	}
	.add-row button,
	.search-form button {
		flex-shrink: 0;
	}
	.catalog-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.catalog-list li {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
	}
	.catalog-list li.active {
		border-color: var(--accent);
	}
	.catalog-list li.suggestion {
		border-style: dashed;
		background: none;
		padding-right: 0.5rem;
	}
	.catalog-link {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		padding: 0.55rem 0.75rem;
		color: var(--fg);
	}
	a.catalog-link:hover {
		text-decoration: none;
	}
	a.catalog-link:hover .catalog-name {
		color: var(--accent);
	}
	.catalog-name {
		font-weight: 600;
		font-size: 0.95rem;
	}
	.catalog-host {
		font-family: ui-monospace, 'SF Mono', monospace;
		font-size: 0.78rem;
		color: var(--muted);
		overflow-wrap: anywhere;
	}
	.catalog-about {
		font-size: 0.8rem;
		color: var(--muted);
	}
	.suggested-head {
		margin-top: 1.75rem;
	}
	.remove {
		background: none;
		border: none;
		color: var(--muted);
		padding: 0.5rem;
		border-radius: 6px;
		margin: 0.2rem;
	}
	.remove:hover {
		filter: none;
		color: var(--danger);
		background: var(--bg);
	}
	.feed {
		transition: opacity 0.15s;
	}
	.feed.loading {
		opacity: 0.45;
	}
	.back {
		display: none;
		align-items: center;
		gap: 0.3rem;
		margin-bottom: 1rem;
		color: var(--muted);
	}
	.pick {
		margin-top: 0.2rem;
	}
	.feed-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
		margin-bottom: 0.75rem;
	}
	.feed-head h2 {
		margin: 0;
		font-size: 1.3rem;
	}
	.start {
		font-size: 0.85rem;
		white-space: nowrap;
	}
	.search-form {
		max-width: 32rem;
	}
	.rights {
		margin-top: 0.9rem;
		max-width: 42rem;
	}
	.entries {
		list-style: none;
		margin: 1rem 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
	}
	.entries > li {
		border-top: 1px solid var(--border);
	}
	.entries > li:last-child {
		border-bottom: 1px solid var(--border);
	}
	.section a {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.7rem 0.25rem;
		color: var(--fg);
	}
	.section a:hover {
		text-decoration: none;
		background: var(--card);
	}
	.section a:hover .title {
		color: var(--accent);
	}
	.section a :global(svg) {
		flex-shrink: 0;
		color: var(--muted);
	}
	.book {
		display: grid;
		grid-template-columns: 4.2rem minmax(0, 1fr);
		gap: 1rem;
		align-items: start;
		padding: 0.9rem 0;
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
	.author,
	.about {
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
	.summary {
		margin: 0.45rem 0 0;
		font-size: 0.85rem;
		max-width: 42rem;
		display: -webkit-box;
		-webkit-line-clamp: 4;
		line-clamp: 4;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.files {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		margin-top: 0.6rem;
	}
	.files button {
		white-space: nowrap;
	}
	.done {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.8rem;
		white-space: nowrap;
	}
	.pager {
		display: flex;
		justify-content: space-between;
		margin-top: 1rem;
		font-size: 0.9rem;
	}
	@media (max-width: 60rem) {
		.layout {
			grid-template-columns: minmax(0, 1fr);
			gap: 1.5rem;
		}
		/* One pane at a time on narrow screens: the list, or one catalog. */
		.layout.has-selection .mine {
			display: none;
		}
		.layout:not(.has-selection) .feed {
			display: none;
		}
		.back {
			display: inline-flex;
		}
	}
</style>
