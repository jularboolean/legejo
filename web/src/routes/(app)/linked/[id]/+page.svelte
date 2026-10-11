<script lang="ts">
	// A shelf followed by a private link: its books as they stand there now,
	// each to be taken into one's own library.
	import { goto } from '$app/navigation';
	import { ArrowLeft, BookPlus, Check, Link } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { t } from '#lib/i18n';
	import type { LinkedBook } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();
	const shelf = $derived(data.linked.shelf);

	/** Per book (by its file's address): fetching, the id it got here, or what went wrong. */
	let taking = $state<Record<string, boolean>>({});
	let taken = $state<Record<string, number>>({});
	let failed = $state<Record<string, string>>({});
	let brokenCovers = $state<Set<string>>(new Set());
	let unfollowOpen = $state(false);

	const cover = (book: LinkedBook) => (book.cover ? `/api/linked-shelves/${shelf.id}/image?url=${encodeURIComponent(book.cover)}` : null);
	const count = $derived(data.linked.books.length === 1 ? t('shelf.book') : t('shelf.books', { count: data.linked.books.length }));

	async function take(book: LinkedBook) {
		const file = book.files[0];
		if (!file || taking[file.href]) return;
		taking = { ...taking, [file.href]: true };
		failed = { ...failed, [file.href]: '' };
		try {
			const res = await fetch(`/api/linked-shelves/${shelf.id}/import`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ href: file.href, title: book.title, format: file.format })
			});
			const body = await res.json().catch(() => null);
			if (res.ok) taken = { ...taken, [file.href]: body.id };
			else if (res.status === 409 && body?.book_id) taken = { ...taken, [file.href]: body.book_id };
			else failed = { ...failed, [file.href]: body?.error === 'missing' ? t('linked.error.missing') : t('linked.takeFailed') };
		} catch {
			failed = { ...failed, [file.href]: t('common.network') };
		}
		taking = { ...taking, [file.href]: false };
	}

	async function unfollow() {
		unfollowOpen = false;
		await fetch(`/api/linked-shelves/${shelf.id}`, { method: 'DELETE' }).catch(() => null);
		await goto('/public', { invalidateAll: true });
	}
</script>

<a class="back" href="/public"><ArrowLeft size={13} /> {t('linked.back')}</a>

<div class="head">
	<div>
		<h1>{shelf.title}</h1>
		<p class="from"><Link size={13} strokeWidth={1.75} /> {t('linked.from', { host: shelf.host })}{#if !data.failed}&ensp;·&ensp;{count}{/if}</p>
		{#if shelf.description}<p class="description">{shelf.description}</p>{/if}
	</div>
	<button type="button" class="ghost" onclick={() => (unfollowOpen = true)}>{t('linked.unfollow')}</button>
</div>

{#if data.failed}
	<p class="notice" role="alert">
		{data.failed === 'missing' || data.failed === 'refused' ? t('linked.error.missing') : t('linked.error.unreachable')}
	</p>
{:else if data.linked.books.length === 0}
	<p class="empty">{t('linked.empty')}</p>
{:else}
	<ul class="books">
		{#each data.linked.books as book (book.files[0]?.href ?? book.title)}
			{@const file = book.files[0]}
			{@const src = cover(book)}
			<li>
				<div class="cover">
					{#if src && !brokenCovers.has(src)}
						<img {src} alt="" loading="lazy" onerror={() => (brokenCovers = new Set([...brokenCovers, src]))} />
					{:else}
						<span>{book.title}</span>
					{/if}
				</div>
				<div class="what">
					<span class="title">{book.title}</span>
					{#if book.authors.length > 0}<span class="author">{book.authors.join(', ')}</span>{/if}
					{#if book.summary}<span class="summary">{book.summary}</span>{/if}
					{#if file && failed[file.href]}<span class="error" role="alert">{failed[file.href]}</span>{/if}
				</div>
				{#if file}
					{#if taken[file.href]}
						<a class="done" href={`/books/${taken[file.href]}`}><Check size={14} /> {t('linked.taken')}</a>
					{:else}
						<button type="button" onclick={() => take(book)} disabled={taking[file.href]}>
							<BookPlus size={14} />
							{taking[file.href] ? t('linked.taking') : t('linked.take')}
						</button>
					{/if}
				{/if}
			</li>
		{/each}
	</ul>
{/if}

<ConfirmDialog
	open={unfollowOpen}
	title={t('linked.unfollow')}
	message={t('linked.unfollowConfirm', { name: shelf.title })}
	confirmLabel={t('linked.unfollow')}
	cancelLabel={t('common.cancel')}
	onconfirm={unfollow}
	oncancel={() => (unfollowOpen = false)}
/>

<style>
	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.88rem;
		color: var(--muted);
		text-decoration: none;
	}
	.head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1rem;
		margin: 0.9rem 0 1.5rem;
		max-width: 46rem;
	}
	h1 {
		font-size: 1.4rem;
		margin: 0 0 0.25rem;
	}
	.from {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		margin: 0;
		font-size: 0.88rem;
		color: var(--muted);
	}
	.description {
		margin: 0.6rem 0 0;
		max-width: 38rem;
	}
	.head button {
		flex: none;
		font-size: 0.85rem;
	}
	.notice,
	.empty {
		color: var(--muted);
		max-width: 38rem;
	}
	.books {
		list-style: none;
		margin: 0;
		padding: 0;
		max-width: 46rem;
	}
	.books li {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0.8rem 0;
		border-bottom: 1px solid var(--border);
	}
	.cover {
		flex: none;
		width: 3.6rem;
		aspect-ratio: 2 / 3;
		border-radius: 4px;
		overflow: hidden;
		background: var(--card);
		display: flex;
		align-items: center;
		justify-content: center;
		box-shadow: var(--shadow);
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
		font-weight: 600;
	}
	.author {
		font-size: 0.88rem;
		color: var(--muted);
	}
	.summary {
		font-size: 0.85rem;
		color: var(--muted);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.error {
		font-size: 0.85rem;
		color: var(--danger);
	}
	.books button,
	.done {
		flex: none;
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.85rem;
		white-space: nowrap;
	}
	.done {
		color: var(--fg);
	}
	@media (max-width: 34rem) {
		.books li {
			flex-wrap: wrap;
		}
		.what {
			flex-basis: calc(100% - 5rem);
		}
		.books button,
		.done {
			margin-left: 4.6rem;
		}
	}
</style>
