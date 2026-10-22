<script lang="ts">
	// What a search finds on shelves of other instances: federated shelves
	// one follows, and shelves shared by a private link. Asked for apart from
	// the search of one's own instance, so that a slow instance never holds
	// that up; each book opens for a closer look and can be taken home.
	import { BookPlus, Check } from '@lucide/svelte';
	import BookPeek from '#lib/BookPeek.svelte';
	import { formatSize } from '#lib/fed';
	import { getLocale, t } from '#lib/i18n';
	import { languageLabel } from '#lib/library';
	import { licenseName } from '#lib/license';
	import type { FedBook, LinkedBook } from '#lib/types';

	/** `found` tells the page how many hits there are, so it can say nothing was found only when that is so. */
	let { q, found = $bindable(0) }: { q: string; found?: number } = $props();

	type LinkedHit = { shelf_id: number; shelf_title: string; host: string; book: LinkedBook };
	type FedHit = { actor: string; shelf_name: string | null; host: string; book: FedBook };
	type Hit = { kind: 'linked'; hit: LinkedHit } | { kind: 'federated'; hit: FedHit };

	let hits = $state<Hit[]>([]);
	let peek = $state<Hit | null>(null);
	/** Per book (its key): the id it got here, being fetched, or what went wrong. */
	let taken = $state<Record<string, number>>({});
	let taking = $state<Record<string, boolean>>({});
	let failed = $state<Record<string, string>>({});

	$effect(() => {
		const query = q.trim();
		hits = [];
		if (!query) return;
		const controller = new AbortController();
		fetch(`/api/search/elsewhere?q=${encodeURIComponent(query)}`, { signal: controller.signal })
			.then((res) => (res.ok ? res.json() : null))
			.then((found: { linked: LinkedHit[]; federated: FedHit[] } | null) => {
				if (!found) return;
				hits = [
					...found.federated.map((hit) => ({ kind: 'federated' as const, hit })),
					...found.linked.map((hit) => ({ kind: 'linked' as const, hit }))
				];
			})
			.catch(() => {});
		return () => controller.abort();
	});

	$effect(() => {
		found = hits.length;
	});

	const key = (h: Hit) => (h.kind === 'federated' ? h.hit.book.object_iri : `${h.hit.shelf_id}:${h.hit.book.files[0]?.href}`);
	const title = (h: Hit) => h.hit.book.title;
	const author = (h: Hit) => (h.kind === 'federated' ? h.hit.book.author : h.hit.book.authors.join(', ') || null);
	const cover = (h: Hit) =>
		h.kind === 'federated'
			? h.hit.book.cover_url
			: h.hit.book.cover
				? `/api/linked-shelves/${h.hit.shelf_id}/image?url=${encodeURIComponent(h.hit.book.cover)}`
				: null;
	const shelfHref = (h: Hit) => (h.kind === 'federated' ? `/fediverse?actor=${encodeURIComponent(h.hit.actor)}` : `/linked/${h.hit.shelf_id}`);
	const shelfName = (h: Hit) => (h.kind === 'federated' ? (h.hit.shelf_name ?? h.hit.host) : h.hit.shelf_title);
	const owned = (h: Hit) => taken[key(h)] ?? (h.kind === 'federated' ? h.hit.book.imported_book_id : null);

	function facts(h: Hit) {
		const lang = h.hit.book.language ? languageLabel(h.hit.book.language, getLocale()) : null;
		if (h.kind === 'federated') {
			const b = h.hit.book;
			return [
				{ label: t('book.series'), value: b.series ? `${b.series}${b.series_index ? ` #${b.series_index}` : ''}` : null },
				{ label: t('book.firstPublished'), value: b.first_published != null ? String(b.first_published) : null },
				{ label: t('book.publisher'), value: b.publisher },
				{ label: t('book.language'), value: lang },
				{ label: t('book.license'), value: b.license ? licenseName(b.license) : null },
				{ label: t('book.format'), value: b.epub_size != null ? `EPUB · ${formatSize(b.epub_size)}` : 'EPUB' }
			];
		}
		const b = h.hit.book;
		const file = b.files[0];
		return [
			{ label: t('book.publisher'), value: b.publisher },
			{ label: t('book.published'), value: b.issued },
			{ label: t('book.language'), value: lang },
			{ label: t('book.format'), value: file ? `${file.format.toUpperCase()}${file.size != null ? ` · ${formatSize(file.size)}` : ''}` : null }
		];
	}

	async function take(h: Hit) {
		const k = key(h);
		if (taking[k]) return;
		taking = { ...taking, [k]: true };
		failed = { ...failed, [k]: '' };
		try {
			const res =
				h.kind === 'federated'
					? await fetch('/api/fed/import', {
							method: 'POST',
							headers: { 'Content-Type': 'application/json' },
							body: JSON.stringify({ object_iri: h.hit.book.object_iri })
						})
					: await fetch(`/api/linked-shelves/${h.hit.shelf_id}/import`, {
							method: 'POST',
							headers: { 'Content-Type': 'application/json' },
							body: JSON.stringify({ href: h.hit.book.files[0]?.href, title: h.hit.book.title, format: h.hit.book.files[0]?.format })
						});
			const body = await res.json().catch(() => null);
			if (res.ok && body?.id != null) taken = { ...taken, [k]: body.id };
			else if (res.status === 409 && body?.book_id != null) taken = { ...taken, [k]: body.book_id };
			else failed = { ...failed, [k]: t('linked.takeFailed') };
		} catch {
			failed = { ...failed, [k]: t('common.network') };
		}
		taking = { ...taking, [k]: false };
	}
</script>

{#snippet takeButton(h: Hit)}
	{@const k = key(h)}
	{#if owned(h) != null}
		<a class="done" href={`/books/${owned(h)}`}><Check size={13} /> {t('linked.taken')}</a>
	{:else if h.kind === 'federated' || h.hit.book.files.length > 0}
		<button type="button" onclick={() => take(h)} disabled={taking[k]}>
			<BookPlus size={13} />
			{taking[k] ? t('linked.taking') : t('linked.take')}
		</button>
	{/if}
	{#if failed[k]}<span class="error" role="alert">{failed[k]}</span>{/if}
{/snippet}

{#if hits.length > 0}
	<section>
		<h2>{t('search.elsewhere')}</h2>
		<ul class="rows">
			{#each hits as h (key(h))}
				{@const src = cover(h)}
				<li>
					<button type="button" class="cover" onclick={() => (peek = h)} aria-label={t('bookPeek.open', { title: title(h) })}>
						{#if src}<img {src} alt="" loading="lazy" referrerpolicy="no-referrer" />{:else}<span>{title(h)}</span>{/if}
					</button>
					<div class="what">
						<button type="button" class="title" onclick={() => (peek = h)}>{title(h)}</button>
						{#if author(h)}<span class="sub">{author(h)}</span>{/if}
						<span class="sub">
							{t('search.elsewhereOn', { shelf: shelfName(h), host: h.hit.host })} · <a href={shelfHref(h)}>{t('search.elsewhereShelf')}</a>
						</span>
					</div>
					<div class="act">{@render takeButton(h)}</div>
				</li>
			{/each}
		</ul>
	</section>
{/if}

{#if peek}
	{@const h = peek}
	<BookPeek
		open
		title={title(h)}
		author={author(h)}
		cover={cover(h)}
		description={h.hit.book.description ?? h.hit.book.summary}
		facts={facts(h)}
		link={h.kind === 'federated' && h.hit.book.page_url
			? { href: h.hit.book.page_url, label: t('bookPeek.onInstance', { host: h.hit.host }) }
			: null}
		onclose={() => (peek = null)}
	>
		{#snippet actions()}{@render takeButton(h)}{/snippet}
	</BookPeek>
{/if}

<style>
	section {
		margin-bottom: 2rem;
	}
	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
		max-width: 46rem;
	}
	.rows li {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0.7rem 0;
		border-bottom: 1px solid var(--border);
	}
	.cover {
		flex: none;
		width: 3.4rem;
		aspect-ratio: 2 / 3;
		padding: 0;
		border: 1px solid var(--border);
		border-radius: 3px;
		overflow: hidden;
		background: var(--card);
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: pointer;
	}
	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.cover span {
		font-size: 0.5rem;
		padding: 0.2rem;
		text-align: center;
		color: var(--muted);
		overflow: hidden;
	}
	.what {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}
	.title {
		align-self: flex-start;
		padding: 0;
		border: none;
		background: none;
		color: var(--fg);
		font: inherit;
		font-weight: 600;
		text-align: left;
		cursor: pointer;
	}
	.title:hover {
		text-decoration: underline;
	}
	.sub {
		font-size: 0.85rem;
		color: var(--muted);
	}
	.act {
		flex: none;
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 0.25rem;
	}
	.done {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.85rem;
	}
	.error {
		font-size: 0.8rem;
		color: var(--danger);
	}
	@media (max-width: 520px) {
		.rows li {
			flex-wrap: wrap;
		}
		.act {
			width: 100%;
			align-items: flex-start;
		}
	}
</style>
