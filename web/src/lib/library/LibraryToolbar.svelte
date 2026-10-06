<script lang="ts">
	import {
		ArrowDown,
		ArrowUp,
		ArrowUpDown,
		Check,
		Search,
		SlidersHorizontal,
		X
	} from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import type { MessageKey } from '#lib/i18n';
	import {
		SORT_KEYS,
		STATUSES,
		activeFilterCount,
		defaultDir,
		emptyFilters,
		type Filters,
		type Prefs,
		type StatusFilter,
		type SortKey
	} from '#lib/library';
	import type { Shelf } from '#lib/types';
	import { fold } from './authors';
	import Popover from './Popover.svelte';
	import ViewSwitch from './ViewSwitch.svelte';

	let {
		filters = $bindable(),
		prefs = $bindable(),
		languages,
		authors,
		shelves,
		unshelvedCount,
		issuesCount = 0
	}: {
		filters: Filters;
		prefs: Prefs;
		/** Languages present in the library, most common first. */
		languages: { code: string; label: string; count: number }[];
		/** Authors in the library, by surname. */
		authors: { key: string; name: string; count: number }[];
		shelves: Shelf[];
		unshelvedCount: number;
		/** Books whose file the health check has remarks on. */
		issuesCount?: number;
	} = $props();

	const sortLabel = (key: SortKey) => t(`library.sort.${key}` as MessageKey);
	const statusLabel = (status: StatusFilter) => t(`library.status.${status}` as MessageKey);

	const activeCount = $derived(activeFilterCount(filters));
	const usedShelves = $derived(shelves.filter((s) => s.book_count > 0));

	function chooseSort(key: SortKey, close: () => void) {
		prefs = { ...prefs, sort: key, dir: defaultDir(key) };
		close();
	}

	// Picking the active value again clears that filter.
	let authorQuery = $state('');
	const shownAuthors = $derived.by(() => {
		const q = fold(authorQuery.trim());
		return q ? authors.filter((a) => fold(a.name).includes(q)) : authors;
	});

	function toggle<K extends 'status' | 'language' | 'shelf' | 'author' | 'file'>(field: K, value: Filters[K]) {
		filters = { ...filters, [field]: filters[field] === value ? null : value };
	}

	const active = $derived.by(() => {
		const chips: { field: 'status' | 'language' | 'shelf' | 'author' | 'file'; label: string }[] = [];
		if (filters.file) chips.push({ field: 'file', label: t('library.file.issues') });
		if (filters.status) chips.push({ field: 'status', label: statusLabel(filters.status) });
		if (filters.language) {
			const lang = languages.find((l) => l.code === filters.language);
			chips.push({ field: 'language', label: lang?.label ?? filters.language });
		}
		if (filters.shelf === 'none') {
			chips.push({ field: 'shelf', label: t('library.shelf.none') });
		} else if (filters.shelf !== null) {
			const shelf = shelves.find((s) => s.id === filters.shelf);
			chips.push({ field: 'shelf', label: shelf?.name ?? '?' });
		}
		if (filters.author) {
			const author = authors.find((a) => a.key === filters.author);
			chips.push({ field: 'author', label: author?.name ?? filters.author });
		}
		return chips;
	});
</script>

<div class="bar">
	<div class="text">
		<Search size={14} class="text-icon" aria-hidden="true" />
		<input
			type="search"
			placeholder={t('library.filterPlaceholder')}
			value={filters.text}
			oninput={(e) => (filters = { ...filters, text: e.currentTarget.value })}
		/>
	</div>

	<div class="controls">
		<div class="sort">
			<Popover label={t('library.sort')}>
				{#snippet button()}
					<ArrowUpDown size={13} />
					<span class="sort-label">{sortLabel(prefs.sort)}</span>
				{/snippet}
				{#snippet children(close)}
					<div role="menu" class="menu">
						{#each SORT_KEYS as key (key)}
							<button
								type="button"
								role="menuitemradio"
								aria-checked={prefs.sort === key}
								class="item"
								onclick={() => chooseSort(key, close)}
							>
								<span class="check">{#if prefs.sort === key}<Check size={13} />{/if}</span>
								{sortLabel(key)}
							</button>
						{/each}
					</div>
				{/snippet}
			</Popover>
			<button
				type="button"
				class="ghost icon"
				title={prefs.dir === 'asc' ? t('library.sort.asc') : t('library.sort.desc')}
				aria-label={prefs.dir === 'asc' ? t('library.sort.asc') : t('library.sort.desc')}
				onclick={() => (prefs = { ...prefs, dir: prefs.dir === 'asc' ? 'desc' : 'asc' })}
			>
				{#if prefs.dir === 'asc'}<ArrowUp size={14} />{:else}<ArrowDown size={14} />{/if}
			</button>
		</div>

		<Popover label={t('library.filter')} align="right">
			{#snippet button()}
				<SlidersHorizontal size={13} />
				{t('library.filter')}
				{#if activeCount > 0}<span class="n">{activeCount}</span>{/if}
			{/snippet}
			{#snippet children()}
				<div class="groups">
					<section>
						<h3>{t('library.filter.status')}</h3>
						<div class="options">
							{#each STATUSES as status (status)}
								<button
									type="button"
									class="option"
									aria-pressed={filters.status === status}
									onclick={() => toggle('status', status)}
								>
									{statusLabel(status)}
								</button>
							{/each}
						</div>
					</section>

					{#if languages.length > 1}
						<section>
							<h3>{t('library.filter.language')}</h3>
							<div class="options">
								{#each languages as lang (lang.code)}
									<button
										type="button"
										class="option"
										aria-pressed={filters.language === lang.code}
										onclick={() => toggle('language', lang.code)}
									>
										{lang.label}
										<span class="num">{lang.count}</span>
									</button>
								{/each}
							</div>
						</section>
					{/if}

					{#if authors.length > 1}
						<section>
							<h3>
								{t('library.filter.author')}
								<span class="h-count">{t('authors.count', { count: authors.length })}</span>
							</h3>
							{#if authors.length > 8}
								<input
									class="author-search"
									type="search"
									placeholder={t('library.filter.authorSearch')}
									bind:value={authorQuery}
								/>
							{/if}
							<div class="author-list">
								{#each shownAuthors as author (author.key)}
									<button
										type="button"
										class="author-option"
										aria-pressed={filters.author === author.key}
										onclick={() => toggle('author', author.key)}
									>
										<span class="author-name">{author.name}</span>
										<span class="num">{author.count}</span>
									</button>
								{:else}
									<p class="none">{t('authors.noMatch')}</p>
								{/each}
							</div>
							<a class="all-authors" href="/authors">{t('authors.showAll')}</a>
						</section>
					{/if}

					{#if usedShelves.length > 0}
						<section>
							<h3>{t('library.filter.shelf')}</h3>
							<div class="options">
								{#each usedShelves as shelf (shelf.id)}
									<button
										type="button"
										class="option"
										aria-pressed={filters.shelf === shelf.id}
										onclick={() => toggle('shelf', shelf.id)}
									>
										{shelf.name}
										<span class="num">{shelf.book_count}</span>
									</button>
								{/each}
								{#if unshelvedCount > 0}
									<button
										type="button"
										class="option dashed"
										aria-pressed={filters.shelf === 'none'}
										onclick={() => toggle('shelf', 'none')}
									>
										{t('library.shelf.none')}
										<span class="num">{unshelvedCount}</span>
									</button>
								{/if}
							</div>
						</section>
					{/if}

					{#if issuesCount > 0 || filters.file}
						<section>
							<h3>{t('library.filter.file')}</h3>
							<div class="options">
								<button
									type="button"
									class="option"
									aria-pressed={filters.file === 'issues'}
									onclick={() => toggle('file', 'issues')}
								>
									{t('library.file.issues')}
									<span class="num">{issuesCount}</span>
								</button>
							</div>
						</section>
					{/if}
				</div>
			{/snippet}
		</Popover>

		<ViewSwitch view={prefs.view} onchange={(view) => (prefs = { ...prefs, view })} />
	</div>
</div>

{#if active.length > 0}
	<div class="active">
		{#each active as chip (chip.field)}
			<button
				type="button"
				class="chip"
				aria-label={t('library.removeFilter', { name: chip.label })}
				onclick={() => (filters = { ...filters, [chip.field]: null })}
			>
				{chip.label}
				<X size={12} />
			</button>
		{/each}
		<button
			type="button"
			class="clear"
			onclick={() => (filters = { ...emptyFilters(), text: filters.text })}
		>
			{t('library.clear')}
		</button>
	</div>
{/if}

<style>
	.bar {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}
	.text {
		position: relative;
		flex: 1;
		min-width: 10rem;
		max-width: 22rem;
	}
	.text :global(.text-icon) {
		position: absolute;
		left: 0.7rem;
		top: 50%;
		transform: translateY(-50%);
		color: var(--muted);
		pointer-events: none;
	}
	.text input {
		padding: 0.38rem 0.7rem 0.38rem 2rem;
		font-size: 0.88rem;
		border-radius: 8px;
	}
	.controls {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-left: auto;
	}
	.sort {
		display: flex;
		align-items: center;
		gap: 0.25rem;
	}
	.icon {
		padding: 0.36rem;
		color: var(--muted);
	}
	.n {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 1.15rem;
		height: 1.15rem;
		padding: 0 0.3rem;
		border-radius: 99px;
		background: var(--accent);
		color: var(--bg);
		font-size: 0.7rem;
		font-weight: 700;
	}

	/* Sort menu */
	.menu {
		display: flex;
		flex-direction: column;
	}
	.item {
		justify-content: flex-start;
		gap: 0.5rem;
		background: none;
		color: var(--fg);
		font-size: 0.86rem;
		font-weight: 500;
		letter-spacing: 0;
		padding: 0.4rem 0.6rem;
		border-radius: 6px;
		text-align: left;
	}
	.item:hover {
		background: var(--bg);
		filter: none;
	}
	.item[aria-checked='true'] {
		color: var(--accent);
		font-weight: 600;
	}
	.check {
		display: inline-flex;
		width: 13px;
	}

	/* Filter panel */
	.groups {
		display: flex;
		flex-direction: column;
		gap: 0.9rem;
		padding: 0.5rem 0.45rem 0.55rem;
		width: min(22rem, 78vw);
	}
	h3 {
		margin: 0 0 0.4rem;
		font-family: inherit;
		font-size: 0.7rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--muted);
	}
	.options {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
	}
	.option {
		background: none;
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 99px;
		font-size: 0.8rem;
		font-weight: 500;
		letter-spacing: 0;
		padding: 0.2rem 0.65rem;
	}
	.option.dashed {
		border-style: dashed;
	}
	.option:hover {
		border-color: var(--accent);
		filter: none;
	}
	.option[aria-pressed='true'] {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--bg);
	}
	.num {
		font-size: 0.72rem;
		font-variant-numeric: tabular-nums;
		opacity: 0.6;
	}

	/* Active filters */
	.active {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
		margin: -0.25rem 0 1rem;
	}
	.chip {
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		color: var(--accent);
		border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
		border-radius: 99px;
		font-size: 0.8rem;
		font-weight: 600;
		letter-spacing: 0;
		padding: 0.15rem 0.5rem 0.15rem 0.7rem;
		gap: 0.3rem;
	}
	.chip:hover {
		background: color-mix(in srgb, var(--accent) 20%, transparent);
		filter: none;
	}
	.clear {
		background: none;
		color: var(--muted);
		font-size: 0.8rem;
		font-weight: 500;
		letter-spacing: 0;
		padding: 0.15rem 0.4rem;
	}
	.clear:hover {
		color: var(--fg);
		filter: none;
		text-decoration: underline;
	}

	@media (max-width: 40rem) {
		.bar {
			flex-wrap: wrap;
		}
		.text {
			flex-basis: 100%;
			max-width: none;
		}
		.controls {
			position: relative;
			margin-left: 0;
			width: 100%;
			justify-content: space-between;
		}
		/* A menu anchored to its own button runs off the screen here: the
		   buttons sit anywhere along the row. Anchor the menus to the row
		   instead, so they span it edge to edge. */
		.bar .controls :global(.popover) {
			position: static;
		}
		.bar .controls :global(.popover .panel) {
			left: 0;
			right: 0;
		}
		.groups {
			width: auto;
		}
		.sort-label {
			max-width: 8rem;
			overflow: hidden;
			text-overflow: ellipsis;
		}
	}
	.h-count {
		font-weight: 400;
		text-transform: none;
		letter-spacing: 0;
		margin-left: 0.3rem;
	}
	.author-search {
		font-size: 0.82rem;
		padding: 0.3rem 0.55rem;
		margin-bottom: 0.4rem;
	}
	.author-list {
		max-height: 13rem;
		overflow-y: auto;
		overscroll-behavior: contain;
		display: flex;
		flex-direction: column;
		margin: 0 -0.35rem;
	}
	.author-option {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		background: none;
		border: none;
		color: var(--fg);
		font: inherit;
		font-size: 0.85rem;
		font-weight: 400;
		text-align: left;
		padding: 0.25rem 0.35rem;
		border-radius: 5px;
	}
	.author-option:hover {
		background: var(--bg);
		filter: none;
	}
	.author-option[aria-pressed='true'] {
		background: var(--accent);
		color: var(--bg);
	}
	.author-option[aria-pressed='true'] .num {
		color: inherit;
	}
	.author-name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.none {
		margin: 0.3rem 0.35rem;
		font-size: 0.82rem;
		color: var(--muted);
		font-style: italic;
	}
	.all-authors {
		display: inline-block;
		margin-top: 0.4rem;
		font-size: 0.8rem;
	}
</style>
