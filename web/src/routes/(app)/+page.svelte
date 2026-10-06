<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import { collectAuthors } from '#lib/library/authors';
	import { CheckSquare, Upload } from '@lucide/svelte';
	import UploadPanel, { type UploadItem } from '#lib/library/UploadPanel.svelte';
	import SelectionBar from '#lib/library/SelectionBar.svelte';
	import { getLocale, t } from '#lib/i18n';
	import {
		activeFilterCount,
		emptyFilters,
		filterBooks,
		languageLabel,
		loadFilters,
		loadPrefs,
		primaryLanguage,
		saveFilters,
		savePrefs,
		sortBooks,
		STATUSES
	} from '#lib/library';
	import BookGrid from '#lib/library/BookGrid.svelte';
	import BookRows from '#lib/library/BookRows.svelte';
	import LibraryToolbar from '#lib/library/LibraryToolbar.svelte';
	import { needsAttention } from '#lib/health';
	import type { Book, HealthIssue } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	let prefs = $state(loadPrefs());
	let filters = $state(loadFilters());

	// /?author=<key> (from the author overview or a book page) opens the
	// library filtered on that author; the address is then tidied back to /,
	// and the filter lives on in the tab like any other.
	$effect(() => {
		const author = page.url.searchParams.get('author');
		const status = page.url.searchParams.get('status');
		if (author || status) {
			filters = {
				...emptyFilters(),
				author: author || null,
				status: STATUSES.find((s) => s === status) ?? null
			};
			goto('/', { replaceState: true });
		}
	});

	const authors = $derived(
		collectAuthors(data.books, getLocale()).map((a) => ({ key: a.key, name: a.name, count: a.books.length }))
	);

	$effect(() => savePrefs(prefs));
	$effect(() => saveFilters(filters));

	// A saved shelf filter may point at a shelf that has since been deleted.
	$effect(() => {
		const shelf = filters.shelf;
		if (typeof shelf === 'number' && !data.shelves.some((s) => s.id === shelf)) {
			filters = { ...filters, shelf: null };
		}
	});

	const languages = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const book of data.books) {
			const code = primaryLanguage(book.language);
			if (code) counts.set(code, (counts.get(code) ?? 0) + 1);
		}
		return [...counts]
			.map(([code, count]) => ({ code, count, label: languageLabel(code, getLocale()) }))
			.sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));
	});
	const unshelvedCount = $derived(data.books.filter((b) => b.shelf_ids.length === 0).length);
	const issuesCount = $derived(data.books.filter((b) => b.health_issues > 0).length);

	const visible = $derived(
		sortBooks(filterBooks(data.books, filters), prefs.sort, prefs.dir, getLocale())
	);
	const narrowed = $derived(activeFilterCount(filters) > 0 || filters.text.trim() !== '');

	// Selection mode: tapping a book selects it; actions in the bar below.
	let selecting = $state(false);
	let selected = $state(new Set<number>());
	function toggleSelect(id: number) {
		const next = new Set(selected);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		selected = next;
	}
	function stopSelecting() {
		selecting = false;
		selected = new Set();
	}
	// Books that disappear (deleted, filtered away) leave the selection.
	$effect(() => {
		const ids = new Set(data.books.map((b) => b.id));
		if ([...selected].some((id) => !ids.has(id))) selected = new Set([...selected].filter((id) => ids.has(id)));
	});

	let fileInput = $state<HTMLInputElement>();
	let uploading = $state(false);
	// One row per file of the current batch, shown until closed.
	let uploads = $state<UploadItem[]>([]);
	let uploadKey = 0;

	type UploadResponse = {
		added: Book[];
		errors: string[];
		duplicates?: Duplicate[];
		reports?: { book_id: number; fixed: string[]; issues: HealthIssue[] }[];
	};

	/** Send one file, reporting how much of it has left the browser. */
	function send(file: File, allowDuplicates: boolean, item: UploadItem): Promise<UploadResponse | null> {
		return new Promise((resolve) => {
			const form = new FormData();
			form.append('files', file);
			const xhr = new XMLHttpRequest();
			xhr.open('POST', `/api/books${allowDuplicates ? '?allow_duplicates=1' : ''}`);
			xhr.upload.onprogress = (e) => {
				if (e.lengthComputable) item.sent = Math.min(file.size, Math.round((e.loaded / e.total) * file.size));
			};
			// Everything is sent; the server is reading and checking the file.
			xhr.upload.onload = () => {
				item.sent = file.size;
				item.state = 'checking';
			};
			xhr.onload = () => {
				if (xhr.status === 413) {
					item.message = t('upload.tooLarge');
					resolve(null);
					return;
				}
				try {
					resolve(xhr.status >= 200 && xhr.status < 300 ? JSON.parse(xhr.responseText) : null);
				} catch {
					resolve(null);
				}
			};
			xhr.onerror = () => {
				item.message = t('common.network');
				resolve(null);
			};
			xhr.send(form);
		});
	}
	let messages = $state<string[]>([]);
	type Duplicate = {
		filename: string;
		kind: 'same_file' | 'same_isbn';
		existing_id: number;
		existing_title: string;
		added_id: number | null;
	};
	let duplicates = $state<Duplicate[]>([]);
	// The files of the last upload, so "upload anyway" can send one again.
	let lastFiles: File[] = [];

	async function uploadFiles(files: FileList | File[] | null, allowDuplicates = false) {
		const epubs = [...(files ?? [])].filter(
			(f) => f.name.toLowerCase().endsWith('.epub') || f.type === 'application/epub+zip'
		);
		if (epubs.length === 0 || uploading) return;
		uploading = true;
		messages = [];
		if (allowDuplicates) {
			const again = new Set(epubs.map((f) => f.name));
			duplicates = duplicates.filter((d) => !again.has(d.filename));
		} else {
			duplicates = [];
			lastFiles = epubs;
		}
		uploads = epubs.map((file) => ({
			key: uploadKey++,
			name: file.name,
			size: file.size,
			state: 'waiting',
			sent: 0,
			fixed: [],
			issues: 0
		}));
		let added = 0;
		try {
			// One file per request: each has its own progress and its own size
			// limit, and one bad file does not stop the others.
			for (const [i, file] of epubs.entries()) {
				const item = uploads[i];
				item.state = 'sending';
				const result = await send(file, allowDuplicates, item);
				if (!result) {
					item.state = 'error';
					item.message ??= t('home.uploadFailed');
					continue;
				}
				const same = (result.duplicates ?? []).find((d) => d.kind === 'same_file');
				duplicates = [...duplicates, ...(result.duplicates ?? [])];
				const book = result.added[0];
				if (book) {
					const report = result.reports?.find((r) => r.book_id === book.id);
					item.state = 'added';
					item.bookId = book.id;
					item.fixed = report?.fixed ?? [];
					item.issues = report?.issues.filter(needsAttention).length ?? 0;
					added++;
				} else if (same) {
					item.state = 'duplicate';
					item.bookId = same.existing_id;
				} else {
					item.state = 'error';
					// The server names the file in front of the reason.
					const reason = (result.errors[0] ?? '').replace(`${file.name}: `, '');
					item.message =
						reason === 'copy-protected'
							? t('upload.drm')
							: reason.startsWith('could not parse epub')
								? t('upload.notEpub')
								: reason || t('home.uploadFailed');
				}
			}
			if (added > 0) await invalidateAll();
		} finally {
			uploading = false;
			if (fileInput) fileInput.value = '';
		}
	}

	// Drag-and-drop over the whole page. dragenter/dragleave fire for every
	// child element, so a counter decides when the overlay shows.
	let dragDepth = $state(0);
	const dragging = $derived(dragDepth > 0);

	function hasFiles(e: DragEvent): boolean {
		return [...(e.dataTransfer?.types ?? [])].includes('Files');
	}
	function ondragenter(e: DragEvent) {
		if (!hasFiles(e)) return;
		e.preventDefault();
		dragDepth += 1;
	}
	function ondragover(e: DragEvent) {
		if (hasFiles(e)) e.preventDefault();
	}
	function ondragleave(e: DragEvent) {
		if (!hasFiles(e)) return;
		dragDepth = Math.max(0, dragDepth - 1);
	}
	function ondrop(e: DragEvent) {
		if (!hasFiles(e)) return;
		e.preventDefault();
		dragDepth = 0;
		uploadFiles([...(e.dataTransfer?.files ?? [])]);
	}
</script>

<svelte:window {ondragenter} {ondragover} {ondragleave} {ondrop} />

{#if dragging}
	<div class="dropzone" aria-hidden="true">
		<span><Upload size={20} /> {t('home.drop')}</span>
	</div>
{/if}

<div class="heading">
	<h1>
		{t('home.title')}
		<span class="count">
			{narrowed
				? t('library.count', { shown: visible.length, total: data.books.length })
				: data.books.length}
		</span>
	</h1>
	{#if data.books.length > 0}
		<button
			type="button"
			class="ghost select-toggle"
			aria-pressed={selecting}
			onclick={() => (selecting ? stopSelecting() : (selecting = true))}
		>
			<CheckSquare size={13} />
			{selecting ? t('select.done') : t('select.start')}
		</button>
	{/if}
	<button class="upload" onclick={() => fileInput?.click()} disabled={uploading}>
		<Upload size={13} />
		{uploading ? t('home.uploading') : t('home.upload')}
	</button>
	<input
		type="file"
		accept=".epub,application/epub+zip"
		multiple
		hidden
		bind:this={fileInput}
		onchange={(e) => uploadFiles(e.currentTarget.files)}
	/>
</div>

{#each messages as msg}
	<p class="error">{msg}</p>
{/each}

{#if uploads.length > 0}
	<UploadPanel items={uploads} onclose={() => (uploads = [])} />
{/if}

{#if duplicates.length > 0}
	<div class="duplicates">
		{#each duplicates as d (d.filename + d.kind)}
			<p>
				{#if d.kind === 'same_file'}
					{t('dup.sameFile', { file: d.filename })}
					<a href={`/books/${d.existing_id}`}>{d.existing_title}</a>.
					<button
						type="button"
						class="link"
						disabled={uploading}
						onclick={() => uploadFiles(lastFiles.filter((f) => f.name === d.filename), true)}
					>
						{t('dup.uploadAnyway')}
					</button>
				{:else}
					{t('dup.sameIsbn', { file: d.filename })}
					<a href={`/books/${d.existing_id}`}>{d.existing_title}</a>
					{#if d.added_id}· <a href={`/books/${d.added_id}`}>{t('dup.seeNew')}</a>{/if}
				{/if}
			</p>
		{/each}
		<button type="button" class="link dismiss" onclick={() => (duplicates = [])}>{t('dup.dismiss')}</button>
	</div>
{/if}

{#if data.books.length === 0}
	<p class="empty">{t('home.empty')}</p>
{:else}
	<LibraryToolbar bind:filters bind:prefs {languages} {authors} shelves={data.shelves} {unshelvedCount} {issuesCount} />

	{#if visible.length === 0}
		<div class="nomatch">
			<p>{t('library.noMatch')}</p>
			<button type="button" class="ghost" onclick={() => (filters = emptyFilters())}>
				{t('library.clearFilters')}
			</button>
		</div>
	{:else if prefs.view === 'list'}
		<BookRows
			books={visible}
			shelves={data.shelves}
			selected={selecting ? selected : null}
			ontoggle={toggleSelect}
		/>
	{:else}
		<BookGrid books={visible} selected={selecting ? selected : null} ontoggle={toggleSelect} />
	{/if}

	{#if selecting}
		<SelectionBar
			bind:selected
			visibleIds={visible.map((b) => b.id)}
			shelves={data.shelves}
			ondone={stopSelecting}
		/>
	{/if}
{/if}

<style>
	.select-toggle {
		margin-left: auto;
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.85rem;
	}
	.select-toggle[aria-pressed='true'] {
		border-color: var(--accent);
		color: var(--accent);
	}
	.duplicates {
		background: var(--card);
		border: 1px solid var(--border);
		border-left: 3px solid var(--gold, var(--accent));
		border-radius: 6px;
		padding: 0.6rem 0.9rem;
		margin-bottom: 1rem;
		font-size: 0.88rem;
	}
	.duplicates p {
		margin: 0 0 0.3rem;
	}
	.link {
		background: none;
		border: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		font-weight: 500;
		cursor: pointer;
	}
	.link:hover {
		text-decoration: underline;
		filter: none;
	}
	.dismiss {
		font-size: 0.8rem;
		color: var(--muted);
	}
	.heading {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		margin-bottom: 1.1rem;
	}
	.upload {
		font-size: 0.78rem;
		padding: 0.32rem 0.7rem;
	}
	.dropzone {
		position: fixed;
		inset: 0;
		z-index: 60;
		display: flex;
		align-items: center;
		justify-content: center;
		background: color-mix(in srgb, var(--bg) 80%, transparent);
		pointer-events: none;
	}
	.dropzone span {
		display: inline-flex;
		align-items: center;
		gap: 0.6rem;
		font-family: 'Fraunces', Georgia, serif;
		font-size: 1.3rem;
		font-weight: 600;
		color: var(--accent);
		border: 2px dashed var(--accent);
		border-radius: 12px;
		padding: 2rem 3rem;
		background: var(--card);
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
		font-variant-numeric: tabular-nums;
	}
	.empty {
		color: var(--muted);
	}
	.nomatch {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		padding: 3rem 1rem;
		color: var(--muted);
		border: 1px dashed var(--border);
		border-radius: 12px;
	}
	.nomatch p {
		margin: 0;
	}
</style>
