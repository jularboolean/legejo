<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { ArrowLeft, ImagePlus, Plus, Search, Trash2 } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { t } from '#lib/i18n';
	import OpenLibraryLookup from '#lib/OpenLibraryLookup.svelte';
	import EditionDate from '#lib/EditionDate.svelte';
	import { normalizePublished, parseYear } from '#lib/pubdate';
	import { LICENSES, federable, licenseName, reasonText } from '#lib/license';
	import type { BlockingBook, LibrisCandidate, Shelf } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	let form = $state({
		title: data.book.title,
		author: data.book.author ?? '',
		language: data.book.language ?? '',
		description: data.book.description ?? '',
		publisher: data.book.publisher ?? '',
		published: data.book.published ?? '',
		first_published: data.book.first_published != null ? String(data.book.first_published) : '',
		category: data.book.category ?? '',
		isbn: data.book.isbn ?? '',
		libris_id: data.book.libris_id ?? '',
		series: data.book.series ?? '',
		series_index: data.book.series_index != null ? String(data.book.series_index) : '',
		tags: data.book.tags.join(', '),
		license: data.book.license ?? '',
		license_source_url: data.book.license_source_url ?? '',
		author_death_year: data.book.author_death_year != null ? String(data.book.author_death_year) : '',
		cover_is_free: data.book.cover_is_free
	});

	// Source and cover only matter for a free licence, the death year only
	// for public domain; hidden fields are not sent.
	const isFree = $derived(form.license !== '' && form.license !== 'copyright');
	const deathYear = $derived(
		/^-?\d{1,4}$/.test(form.author_death_year.trim()) ? Number(form.author_death_year.trim()) : null
	);
	const status = $derived(federable(form.license, form.license_source_url, deathYear));

	const LICENSE_ERRORS: Record<string, Parameters<typeof t>[0]> = {
		'unknown licence': 'license.error.unknown',
		'the source must be an http(s) address': 'license.error.source',
		'the year of death is not plausible': 'license.error.deathYear',
		'invalid published date': 'edit.editionInvalid',
		'invalid first published year': 'edit.firstPublishedInvalid'
	};
	let shelves = $state<Shelf[]>([...data.shelves]);
	let selectedShelves = $state<Set<number>>(new Set(data.book.shelves.map((s) => s.id)));
	let newShelfName = $state('');
	let saving = $state(false);
	let errorMsg = $state('');
	// Federation refusals: why the save was stopped, one line per book.
	let blocking = $state<BlockingBook[]>([]);

	let hasCover = $state(data.book.has_cover);
	let coverVersion = $state(0);
	let coverMsg = $state('');
	let coverError = $state(false);
	let coverInput: HTMLInputElement;

	async function uploadCover(files: FileList | null) {
		if (!files || files.length === 0) return;
		coverMsg = '';
		coverError = false;
		const body = new FormData();
		body.append('cover', files[0]);
		try {
			const res = await fetch(`/api/books/${data.book.id}/cover`, { method: 'POST', body });
			if (!res.ok) {
				const err = await res.json().catch(() => null);
				coverMsg = err?.error ?? t('cover.failed');
				coverError = true;
				return;
			}
			const result: { epub_updated: boolean } = await res.json();
			hasCover = true;
			coverVersion++;
			coverMsg =
				data.book.format !== 'epub'
					? t('cover.updated')
					: result.epub_updated
						? t('cover.updatedEpub')
						: t('cover.updatedCatalogOnly');
			await invalidateAll();
		} catch {
			coverMsg = t('common.network');
			coverError = true;
		} finally {
			coverInput.value = '';
		}
	}

	let authorSuggestions = $state<string[]>([]);
	let authorDebounce: ReturnType<typeof setTimeout>;

	function onAuthorInput() {
		clearTimeout(authorDebounce);
		authorDebounce = setTimeout(async () => {
			const q = form.author.trim();
			if (q.length < 2) {
				authorSuggestions = [];
				return;
			}
			try {
				const res = await fetch(`/api/authors?q=${encodeURIComponent(q)}`);
				if (res.ok) authorSuggestions = await res.json();
			} catch {
				// Suggestions are a convenience; typing goes on without them.
			}
		}, 200);
	}

	let librisLoading = $state(false);
	let librisMsg = $state('');
	let librisRecords = $state(0);
	let candidates = $state<LibrisCandidate[]>([]);

	async function searchLibris() {
		librisLoading = true;
		librisMsg = '';
		candidates = [];
		librisRecords = 0;
		const params = new URLSearchParams();
		if (form.isbn.trim()) params.set('isbn', form.isbn);
		if (form.title.trim()) params.set('title', form.title);
		if (form.author.trim()) params.set('author', form.author);
		try {
			const res = await fetch(`/api/libris/search?${params}`);
			if (!res.ok) {
				librisMsg = t('libris.failed');
				return;
			}
			const result: { records: number; candidates: LibrisCandidate[] } = await res.json();
			candidates = result.candidates;
			librisRecords = result.records;
			if (candidates.length === 0) librisMsg = t('libris.none');
		} catch {
			librisMsg = t('common.network');
		} finally {
			librisLoading = false;
		}
	}

	/** "Lagerlöf, Selma, 1858-1940" -> "Selma Lagerlöf" */
	function displayName(creator: string): string {
		const parts = creator.split(',').map((p) => p.trim());
		if (parts.length >= 2 && !/\d/.test(parts[1])) return `${parts[1]} ${parts[0]}`;
		return parts[0];
	}

	/** "Stockholm : Bonnier" -> "Bonnier" */
	function displayPublisher(publisher: string): string {
		const i = publisher.lastIndexOf(' : ');
		return i >= 0 ? publisher.slice(i + 3).trim() : publisher;
	}

	async function applyCandidate(c: LibrisCandidate) {
		if (c.creator) form.author = displayName(c.creator);
		if (c.publisher) form.publisher = displayPublisher(c.publisher);
		if (c.date) form.published = normalizePublished(c.date) ?? form.published;
		if (c.language) form.language = c.language;
		if (c.isbn.length > 0 && !form.isbn.trim()) form.isbn = c.isbn[0];
		if (c.libris_id) form.libris_id = c.libris_id;
		candidates = [];
		librisMsg = t('libris.applied');

		// Pull the record's summary too; fill the description if it's empty.
		if (c.libris_id && !form.description.trim()) {
			try {
				const res = await fetch(`/api/libris/summary?id=${encodeURIComponent(c.libris_id)}`);
				if (res.ok) {
					const { summary }: { summary: string | null } = await res.json();
					if (summary && !form.description.trim()) {
						form.description = summary;
						librisMsg = t('libris.appliedSummary');
					}
				}
			} catch {
				// Summary is a bonus; the applied fields stand on their own.
			}
		}
	}

	function toggleShelf(id: number) {
		const next = new Set(selectedShelves);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		selectedShelves = next;
	}

	async function addShelf() {
		const name = newShelfName.trim();
		if (!name) return;
		errorMsg = '';
		try {
			const res = await fetch('/api/shelves', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ name })
			});
			if (res.status === 409) {
				errorMsg = t('edit.shelfExists');
				return;
			}
			if (!res.ok) {
				errorMsg = t('edit.saveFailed');
				return;
			}
			const shelf: Shelf = await res.json();
			shelves = [...shelves, shelf].sort((a, b) => a.name.localeCompare(b.name));
			selectedShelves = new Set([...selectedShelves, shelf.id]);
			newShelfName = '';
		} catch {
			errorMsg = t('common.network');
		}
	}

	let deleteOpen = $state(false);

	async function removeBook() {
		deleteOpen = false;
		const res = await fetch(`/api/books/${data.book.id}`, { method: 'DELETE' });
		if (res.ok) {
			await invalidateAll();
			goto('/');
		} else {
			errorMsg = t('edit.saveFailed');
		}
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		errorMsg = '';
		blocking = [];
		if (parseYear(form.first_published) === undefined) {
			errorMsg = t('edit.firstPublishedInvalid');
			return;
		}
		if (form.license === 'pd' && form.author_death_year.trim() && deathYear == null) {
			errorMsg = t('license.error.deathYear');
			return;
		}
		saving = true;
		try {
			const res = await fetch(`/api/books/${data.book.id}`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					title: form.title,
					author: form.author || null,
					language: form.language || null,
					description: form.description || null,
					publisher: form.publisher || null,
					published: form.published || null,
					first_published: parseYear(form.first_published) ?? null,
					category: form.category || null,
					isbn: form.isbn || null,
					libris_id: form.libris_id || null,
					series: form.series || null,
					series_index: form.series_index.trim()
						? Number(form.series_index.replace(',', '.'))
						: null,
					tags: form.tags
						.split(',')
						.map((t) => t.trim())
						.filter((t) => t.length > 0),
					shelf_ids: [...selectedShelves],
					license: {
						license: form.license || null,
						source_url: isFree ? form.license_source_url.trim() || null : null,
						author_death_year: form.license === 'pd' ? deathYear : null,
						cover_is_free: isFree && form.cover_is_free
					}
				})
			});
			if (res.ok) {
				await invalidateAll();
				goto(`/books/${data.book.id}`);
			} else {
				const body = await res.json().catch(() => null);
				const licenseError = LICENSE_ERRORS[body?.error];
				if (body?.error === 'books not federable') {
					errorMsg = t('fed.bookEdit.notFederable');
					blocking = body.blocking ?? [];
					return;
				}
				if (body?.error === 'book on federated shelf') {
					const names: string[] = body.shelves ?? [];
					errorMsg =
						t('fed.bookEdit.onFederatedShelf', { shelves: names.map((n) => `“${n}”`).join(', ') }) +
						(body.reason ? ' ' + reasonText(body.reason) : '');
					return;
				}
				errorMsg =
					body?.error === 'title must not be empty'
						? t('edit.titleRequired')
						: licenseError
							? t(licenseError)
							: t('edit.saveFailed');
			}
		} catch {
			errorMsg = t('common.network');
		} finally {
			saving = false;
		}
	}
</script>

<a class="back" href={`/books/${data.book.id}`}><ArrowLeft size={13} /> {data.book.title}</a>

<h1>{t('edit.heading')}</h1>

<form onsubmit={save}>
	<div class="cover-section">
		<span class="cover-label">{t('cover.label')}</span>
		<div class="cover-row">
			{#if hasCover}
				<img
					class="cover-preview"
					src={`/api/books/${data.book.id}/cover?v=${coverVersion}-${data.book.updated_at ?? data.book.created_at}`}
					alt=""
				/>
			{/if}
			<div class="cover-side">
				<button type="button" class="ghost" onclick={() => coverInput.click()}>
					<ImagePlus size={13} />
					{t('cover.choose')}
				</button>
				<span class="hint">{data.book.format === 'epub' ? t('cover.hint') : t('cover.hintCatalog')}</span>
			</div>
		</div>
		{#if coverMsg}<p class={coverError ? 'error' : 'cover-msg'}>{coverMsg}</p>{/if}
		<input
			type="file"
			accept="image/*"
			hidden
			bind:this={coverInput}
			onchange={(e) => uploadCover(e.currentTarget.files)}
		/>
	</div>

	<label>
		{t('edit.title')}
		<input bind:value={form.title} required />
	</label>
	<label>
		{t('edit.author')}
		<input bind:value={form.author} oninput={onAuthorInput} list="author-suggestions" />
		<datalist id="author-suggestions">
			{#each authorSuggestions as author (author)}
				<option value={author}></option>
			{/each}
		</datalist>
	</label>
	<div class="row">
		<label>
			{t('edit.language')}
			<input bind:value={form.language} placeholder="en" />
		</label>
		<label>
			{t('edit.firstPublished')}
			<input
				bind:value={form.first_published}
				inputmode="numeric"
				placeholder="1879"
				aria-invalid={parseYear(form.first_published) === undefined}
			/>
			{#if parseYear(form.first_published) === undefined}
				<span class="field-error">{t('edit.firstPublishedInvalid')}</span>
			{:else if !form.first_published.trim() && form.published}
				<button type="button" class="suggest" onclick={() => (form.first_published = form.published.slice(0, 4))}>
					{t('edit.useEditionYear', { year: form.published.slice(0, 4) })}
				</button>
			{:else}
				<span class="hint">{t('edit.firstPublishedHint')}</span>
			{/if}
		</label>
	</div>
	<div class="row">
		<label>
			{t('edit.edition')}
			<EditionDate bind:value={form.published} />
		</label>
		<span></span>
	</div>
	<div class="row">
		<label>
			{t('edit.publisher')}
			<input bind:value={form.publisher} />
		</label>
		<label>
			{t('edit.category')}
			<input bind:value={form.category} placeholder={t('edit.categoryPlaceholder')} />
		</label>
	</div>
	<div class="row">
		<label>
			{t('edit.isbn')}
			<input bind:value={form.isbn} placeholder="9789100123456" />
		</label>
		<label>
			{t('edit.librisId')}
			<input bind:value={form.libris_id} placeholder="491556" />
		</label>
	</div>
	<div class="row series-row">
		<label>
			{t('edit.series')}
			<input bind:value={form.series} placeholder={t('edit.seriesPlaceholder')} />
		</label>
		<label>
			{t('edit.seriesIndex')}
			<input bind:value={form.series_index} inputmode="decimal" placeholder="1" />
		</label>
	</div>
	<label>
		{t('edit.tags')}
		<input bind:value={form.tags} placeholder={t('edit.tagsPlaceholder')} />
		<span class="hint">{t('edit.tagsHint')}</span>
	</label>

	{#if data.librisEnabled}
	<fieldset>
		<legend>{t('libris.legend')}</legend>
		<div class="libris-head">
			<button type="button" class="ghost" onclick={searchLibris} disabled={librisLoading}>
				<Search size={13} />
				{librisLoading ? t('libris.searching') : t('libris.search')}
			</button>
			<span class="hint">{t('libris.hint')}</span>
		</div>
		{#if librisRecords > candidates.length && candidates.length > 0}
			<p class="hint">{t('libris.records', { count: librisRecords, shown: candidates.length })}</p>
		{/if}
		{#if candidates.length > 0}
			<ul class="candidates">
				{#each candidates as c, i (i)}
					<li>
						<button type="button" class="candidate" onclick={() => applyCandidate(c)}>
							<span class="c-title">{c.title ?? '—'}</span>
							<span class="c-meta">
								{[c.creator, c.publisher, c.date, c.isbn[0]].filter(Boolean).join(' · ')}
							</span>
						</button>
					</li>
				{/each}
			</ul>
		{/if}
		{#if librisMsg}<p class="libris-msg">{librisMsg}</p>{/if}
	</fieldset>
	{/if}
	{#if data.openLibraryEnabled}
		<OpenLibraryLookup
			bookId={data.book.id}
			bind:form
			oncover={async () => {
				hasCover = true;
				coverVersion++;
				await invalidateAll();
			}}
		/>
	{/if}
	<label>
		{t('edit.description')}
		<textarea bind:value={form.description} rows="8"></textarea>
		<span class="hint">{t('edit.descriptionHint')}</span>
	</label>

	<fieldset class="license">
		<legend>{t('license.legend')}</legend>
		<label>
			{t('license.label')}
			<select bind:value={form.license}>
				<option value="">{t('license.unknown')}</option>
				{#each LICENSES as license (license)}
					<option value={license}>{licenseName(license)}</option>
				{/each}
			</select>
		</label>
		{#if isFree}
			<label>
				{t('license.source')}
				<input
					type="url"
					bind:value={form.license_source_url}
					placeholder="https://runeberg.org/…"
				/>
				<span class="hint">{t('license.sourceHint')}</span>
			</label>
			{#if form.license === 'pd'}
				<label class="year">
					{t('license.deathYear')}
					<input bind:value={form.author_death_year} inputmode="numeric" placeholder="1912" />
					<span class="hint">{t('license.deathYearHint')}</span>
				</label>
			{/if}
			<label class="check">
				<input type="checkbox" bind:checked={form.cover_is_free} />
				{t('license.coverIsFree')}
			</label>
		{/if}
		<p class="status" class:ok={status.ok}>
			{status.ok ? t('license.federable') : reasonText(status)}
		</p>
		<span class="hint">{t('license.hint')}</span>
	</fieldset>

	<fieldset>
		<legend>{t('edit.shelves')}</legend>
		<div class="shelves">
			{#each shelves as shelf (shelf.id)}
				<label class="shelf">
					<input
						type="checkbox"
						checked={selectedShelves.has(shelf.id)}
						onchange={() => toggleShelf(shelf.id)}
					/>
					{shelf.name}
				</label>
			{/each}
		</div>
		<div class="new-shelf">
			<input
				bind:value={newShelfName}
				placeholder={t('edit.newShelfPlaceholder')}
				onkeydown={(e) => {
					if (e.key === 'Enter') {
						e.preventDefault();
						addShelf();
					}
				}}
			/>
			<button type="button" class="ghost" onclick={addShelf}><Plus size={13} /> {t('edit.addShelf')}</button>
		</div>
	</fieldset>

	{#if errorMsg}<p class="error">{errorMsg}</p>{/if}
	{#if blocking.length > 0}
		<ul class="blocking">
			{#each blocking as b (b.id)}
				<li>
					{#if b.id === data.book.id}
						{reasonText(b.reason)}
					{:else}
						<a href={`/books/${b.id}/edit`}>{b.title}</a>: {reasonText(b.reason)}
					{/if}
				</li>
			{/each}
		</ul>
	{/if}

	<div class="actions">
		<button type="submit" disabled={saving}>{saving ? t('edit.saving') : t('edit.save')}</button>
		<a class="cancel" href={`/books/${data.book.id}`}>{t('common.cancel')}</a>
	</div>
</form>

<div class="danger-zone">
	<button type="button" class="ghost danger" onclick={() => (deleteOpen = true)}>
		<Trash2 size={13} />
		{t('book.delete')}
	</button>
	<span class="hint">{t('edit.deleteHint')}</span>
</div>

<ConfirmDialog
	open={deleteOpen}
	title={t('book.delete')}
	message={t('book.deleteConfirm', { title: data.book.title })}
	confirmLabel={t('book.delete')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={removeBook}
	oncancel={() => (deleteOpen = false)}
/>

<style>
	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		margin-bottom: 1rem;
		color: var(--muted);
	}
	h1 {
		font-size: 1.4rem;
		margin: 0 0 1.25rem;
	}
	form {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		max-width: 38rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.9rem;
		color: var(--muted);
	}
	.hint {
		font-size: 0.75rem;
	}
	textarea {
		font: inherit;
		background: var(--card);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 0.5rem 0.75rem;
		resize: vertical;
	}
	textarea:focus {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}
	.row {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1rem;
	}
	.series-row {
		grid-template-columns: 2fr 1fr;
	}
	fieldset {
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 0.75rem 1rem 1rem;
	}
	legend {
		font-size: 0.9rem;
		color: var(--muted);
		padding: 0 0.3rem;
	}
	.shelves {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem 1.25rem;
		margin-bottom: 0.75rem;
	}
	.shelf {
		flex-direction: row;
		align-items: center;
		gap: 0.4rem;
		color: var(--fg);
	}
	.license {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	.license select {
		font: inherit;
		background: var(--card);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 0.45rem 0.7rem;
		max-width: 18rem;
	}
	.license .year input {
		max-width: 8rem;
	}
	.check {
		flex-direction: row;
		align-items: center;
		gap: 0.4rem;
		color: var(--fg);
	}
	.status {
		margin: 0;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.status.ok {
		color: var(--accent);
	}
	.new-shelf {
		display: flex;
		gap: 0.5rem;
		max-width: 18rem;
	}
	.cover-section {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.cover-label {
		font-size: 0.9rem;
		color: var(--muted);
	}
	.cover-row {
		display: flex;
		gap: 1rem;
		align-items: start;
	}
	.cover-preview {
		width: 6.5rem;
		aspect-ratio: 2 / 3;
		object-fit: cover;
		border-radius: 3px 6px 6px 3px;
		border: 1px solid var(--border);
		box-shadow: var(--shadow);
		flex-shrink: 0;
	}
	.cover-side {
		display: flex;
		flex-direction: column;
		align-items: start;
		gap: 0.4rem;
	}
	.cover-msg {
		margin: 0;
		font-size: 0.85rem;
		color: var(--accent);
	}
	.libris-head {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
	}
	.candidates {
		list-style: none;
		margin: 0.75rem 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.candidate {
		width: 100%;
		display: flex;
		flex-direction: column;
		align-items: start;
		gap: 0.1rem;
		text-align: left;
		background: var(--card);
		color: var(--fg);
		border: 1px solid var(--border);
		padding: 0.5rem 0.7rem;
	}
	.candidate:hover {
		border-color: var(--accent);
		filter: none;
	}
	.c-title {
		font-weight: 600;
		font-size: 0.88rem;
	}
	.c-meta {
		font-size: 0.78rem;
		color: var(--muted);
		font-weight: 400;
	}
	.libris-msg {
		margin: 0.6rem 0 0;
		font-size: 0.85rem;
		color: var(--accent);
	}
	.actions {
		display: flex;
		align-items: center;
		gap: 1rem;
	}
	.cancel {
		color: var(--muted);
	}
	.blocking {
		margin: -0.5rem 0 0;
		padding-left: 1.2rem;
		font-size: 0.85rem;
		color: var(--danger);
	}
	.danger-zone {
		margin-top: 2.5rem;
		padding-top: 1.25rem;
		border-top: 1px solid var(--border);
		max-width: 38rem;
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}
	.danger {
		color: var(--danger);
		border-color: var(--danger);
		flex-shrink: 0;
	}
	.danger-zone .hint {
		font-size: 0.8rem;
		color: var(--muted);
	}
	.suggest {
		align-self: start;
		background: none;
		border: 1px dashed var(--border);
		color: var(--accent);
		font-size: 0.75rem;
		font-weight: 500;
		padding: 0.15rem 0.5rem;
	}
	.suggest:hover {
		border-color: var(--accent);
		filter: none;
	}
	.field-error {
		font-size: 0.75rem;
		color: var(--danger);
	}
</style>
