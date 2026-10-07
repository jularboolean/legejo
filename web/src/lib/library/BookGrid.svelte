<script lang="ts">
	import { Check, Download } from '@lucide/svelte';
	import { getLocale, t } from '#lib/i18n';
	import { languageName } from '#lib/library';
	import type { Book } from '#lib/types';

	let {
		books,
		showLanguage = false,
		selected = null,
		ontoggle
	}: {
		books: Book[];
		/** Name each book's language under its author. */
		showLanguage?: boolean;
		/** Selection mode when set: a tap selects instead of opening the book. */
		selected?: Set<number> | null;
		ontoggle?: (id: number) => void;
	} = $props();
</script>

<div class="grid">
	{#each books as book (book.id)}
		<div class="card" class:picked={selected?.has(book.id)}>
			{#if selected}
				<button
					type="button"
					class="pick"
					aria-pressed={selected.has(book.id)}
					aria-label={book.title}
					onclick={() => ontoggle?.(book.id)}
				>
					<span class="tick">{#if selected.has(book.id)}<Check size={14} strokeWidth={3} />{/if}</span>
				</button>
			{/if}
			<a class="cover" href={`/books/${book.id}`}>
				{#if book.has_cover}
					<img
						src={`/api/books/${book.id}/cover?size=thumb&v=${encodeURIComponent(book.updated_at ?? book.created_at)}`}
						alt={book.title}
						loading="lazy"
					/>
				{:else}
					<span class="placeholder">{book.title}</span>
				{/if}
				{#if book.format && book.format !== 'epub'}<span class="format">{book.format.toUpperCase()}</span>{/if}
				{#if book.progress_percent != null}
					{@const percent = Math.round(book.progress_percent * 100)}
					<span class="read-progress" title={t('book.progress', { percent })}>
						<span style:width={`${percent}%`}></span>
					</span>
				{/if}
			</a>
			<div class="meta">
				<span class="title" title={book.title}>{book.title}</span>
				{#if book.author}<span class="author">{book.author}</span>{/if}
				{#if showLanguage && languageName(book.language, getLocale())}
					<span class="author">{languageName(book.language, getLocale())}</span>
				{/if}
			</div>
			<a
				class="download"
				href={`/api/books/${book.id}/file`}
				title={t('home.download')}
				aria-label={t('home.download')}
			>
				<Download size={13} />
			</a>
		</div>
	{/each}
</div>

<style>
	/* Selection mode: one button over the whole card catches the tap. */
	.pick {
		position: absolute;
		inset: -0.3rem;
		z-index: 3;
		background: transparent;
		border: 2px solid transparent;
		border-radius: 8px;
		padding: 0;
		cursor: pointer;
	}
	.pick:hover {
		filter: none;
		border-color: var(--border);
	}
	.picked .pick {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 10%, transparent);
	}
	.tick {
		position: absolute;
		top: 0.55rem;
		left: 0.55rem;
		width: 1.4rem;
		height: 1.4rem;
		border-radius: 50%;
		display: grid;
		place-items: center;
		background: rgb(255 255 255 / 0.85);
		border: 1.5px solid var(--muted);
		color: var(--bg);
	}
	.picked .tick {
		background: var(--accent);
		border-color: var(--accent);
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr));
		gap: 1.25rem;
	}
	.card {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.read-progress {
		position: absolute;
		inset: auto 0 0 0;
		height: 4px;
		background: rgba(0, 0, 0, 0.35);
	}
	.read-progress span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.cover {
		position: relative;
		aspect-ratio: 2 / 3;
		border-radius: 3px 6px 6px 3px;
		overflow: hidden;
		background: var(--card);
		border: 1px solid var(--border);
		display: flex;
		box-shadow: var(--shadow);
	}
	/* Told only for the formats that cannot be read here. */
	.format {
		position: absolute;
		top: 0.4rem;
		left: 0.4rem;
		padding: 0.05rem 0.4rem;
		border-radius: 4px;
		background: color-mix(in srgb, var(--fg) 78%, transparent);
		color: var(--bg);
		font-size: 0.66rem;
		font-weight: 700;
		letter-spacing: 0.04em;
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
	.meta {
		display: flex;
		flex-direction: column;
	}
	.title {
		font-size: 0.9rem;
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.author {
		font-size: 0.8rem;
		color: var(--muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.download {
		position: absolute;
		top: 0.35rem;
		right: 0.35rem;
		display: inline-flex;
		padding: 0.3rem;
		border-radius: 6px;
		border: 1px solid var(--border);
		background: var(--bg);
		color: var(--muted);
		opacity: 0;
		transition: opacity 0.15s;
	}
	.download:hover {
		color: var(--fg);
		border-color: var(--muted);
	}
	.card:hover .download,
	.download:focus-visible {
		opacity: 1;
	}
</style>
