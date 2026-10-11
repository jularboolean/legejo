<script lang="ts">
	// The books saved on this device. Also where the app lands when the
	// server cannot be reached: these are the books that can be read then.
	import { goto } from '$app/navigation';
	import { BookOpen, RefreshCw, Trash2, WifiOff } from '@lucide/svelte';
	import { formatBytes } from '#lib/audio';
	import { t } from '#lib/i18n';
	import { availability, removeBook, room, savedBooks, type SavedBook } from '#lib/offline/store';
	import { localPercent } from '#lib/reader/progress';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const available = availability();
	let books = $state<SavedBook[] | null>(null);
	let used = $state<number | null>(null);

	async function refresh() {
		books = await savedBooks().catch(() => []);
		used = books.reduce((sum, b) => sum + b.size, 0);
		// The browser's own figure covers the app's files too; the sum of the books is what the reader asks about.
		if (!used) used = (await room())?.used ?? null;
	}
	$effect(() => {
		refresh();
	});

	async function remove(saved: SavedBook) {
		await removeBook(saved.book.id).catch(() => {});
		await refresh();
	}

	let trying = $state(false);
	async function retry() {
		trying = true;
		// The start page asks the server again; without it the way leads back here.
		await goto('/', { invalidateAll: true }).catch(() => {});
		trying = false;
	}

	function percent(saved: SavedBook): number | null {
		const p = localPercent(saved.book.id) ?? saved.book.progress_percent ?? null;
		return p === null ? null : Math.round(p * 100);
	}
</script>

<h1>{t('offline.title')}</h1>

{#if data.offline}
	<div class="unreachable" role="status">
		<WifiOff size={16} />
		<span>{t('offline.banner')}</span>
		<button type="button" onclick={retry} disabled={trying}>
			<RefreshCw size={13} />
			{t('offline.retry')}
		</button>
	</div>
{/if}

{#if available === 'unsupported'}
	<p class="intro">{t('offline.unsupported')}</p>
{:else}
	<p class="intro">{t('offline.intro')}</p>
	{#if available === 'install'}<p class="intro">{t('offline.installHint')}</p>{/if}

	{#if books === null}
		<p class="intro" aria-busy="true"></p>
	{:else if books.length === 0}
		<p class="empty">{t('offline.empty')}</p>
	{:else}
		<ul class="books">
			{#each books as saved (saved.book.id)}
				{@const done = percent(saved)}
				<li>
					<a class="cover" href={`/books/${saved.book.id}/read`} tabindex="-1" aria-hidden="true">
						{#if saved.book.has_cover}
							<img src={`/api/books/${saved.book.id}/cover`} alt="" loading="lazy" />
						{:else}
							<span>{saved.book.title}</span>
						{/if}
					</a>
					<div class="what">
						<a class="title" href={`/books/${saved.book.id}/read`}>{saved.book.title}</a>
						{#if saved.book.author}<span class="author">{saved.book.author}</span>{/if}
						<span class="meta">
							{formatBytes(saved.size)}{#if done !== null}&ensp;·&ensp;{t('book.progress', { percent: done })}{/if}
						</span>
					</div>
					<div class="do">
						<a class="read" href={`/books/${saved.book.id}/read`}>
							<BookOpen size={14} />
							{done !== null ? t('book.continue') : t('book.read')}
						</a>
						<button type="button" class="remove" onclick={() => remove(saved)} aria-label={t('offline.remove')} title={t('offline.remove')}>
							<Trash2 size={15} />
						</button>
					</div>
				</li>
			{/each}
		</ul>
		{#if used}<p class="room">{t('offline.room', { used: formatBytes(used) })}</p>{/if}
	{/if}
{/if}

<style>
	h1 {
		margin: 0 0 0.75rem;
	}
	.intro,
	.empty,
	.room {
		max-width: 38rem;
		color: var(--muted);
		font-size: 0.92rem;
	}
	.empty {
		margin-top: 1.5rem;
	}
	.unreachable {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.6rem;
		max-width: 38rem;
		margin: 0 0 1rem;
		padding: 0.6rem 0.8rem;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--card);
		font-size: 0.92rem;
	}
	.unreachable span {
		flex: 1;
		min-width: 12rem;
	}
	.unreachable button {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.85rem;
	}
	.books {
		list-style: none;
		margin: 1.25rem 0 0;
		padding: 0;
		max-width: 44rem;
	}
	.books li {
		display: flex;
		align-items: center;
		gap: 0.9rem;
		padding: 0.7rem 0;
		border-bottom: 1px solid var(--border);
	}
	.cover {
		flex: none;
		width: 3rem;
		aspect-ratio: 2 / 3;
		border-radius: 4px;
		overflow: hidden;
		background: var(--card);
		display: flex;
		align-items: center;
		justify-content: center;
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
		gap: 0.1rem;
	}
	.title {
		font-weight: 600;
		color: var(--fg);
		text-decoration: none;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.author,
	.meta {
		font-size: 0.85rem;
		color: var(--muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.do {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		flex: none;
	}
	.read {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		padding: 0.4rem 0.7rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		color: var(--fg);
		text-decoration: none;
		font-size: 0.85rem;
		font-weight: 500;
	}
	.remove {
		display: inline-flex;
		padding: 0.45rem;
		background: none;
		border: none;
		color: var(--muted);
	}
	@media (max-width: 30rem) {
		.read {
			padding: 0.4rem 0.55rem;
		}
	}
</style>
