<script lang="ts">
	import { ImageDown, Search } from '@lucide/svelte';
	import { t } from '#lib/i18n';
	import { primaryLanguage } from '#lib/library';

	type Form = {
		title: string;
		author: string;
		language: string;
		description: string;
		publisher: string;
		published: string;
		first_published: string;
		isbn: string;
	};
	type Candidate = {
		work: string | null;
		title: string | null;
		author: string | null;
		publisher: string | null;
		year: number | null;
		isbn: string[];
		language: string | null;
		cover_id: number | null;
	};

	let {
		bookId,
		form = $bindable(),
		oncover
	}: {
		bookId: number;
		form: Form;
		/** A cover was taken from Open Library. */
		oncover: () => void;
	} = $props();

	let loading = $state(false);
	let message = $state('');
	let found = $state(0);
	let candidates = $state<Candidate[]>([]);
	let coverBusy = $state<number | null>(null);

	async function search() {
		loading = true;
		message = '';
		candidates = [];
		const params = new URLSearchParams();
		if (form.isbn.trim()) params.set('isbn', form.isbn);
		else {
			if (form.title.trim()) params.set('title', form.title);
			if (form.author.trim()) params.set('author', form.author);
		}
		try {
			const res = await fetch(`/api/openlibrary/search?${params}`);
			if (!res.ok) {
				message = t('ol.failed');
				return;
			}
			const result: { found: number; candidates: Candidate[] } = await res.json();
			candidates = result.candidates;
			found = result.found;
			if (candidates.length === 0) message = t('ol.none');
		} catch {
			message = t('common.network');
		} finally {
			loading = false;
		}
	}

	async function apply(c: Candidate) {
		if (c.author) form.author = c.author;
		if (c.publisher) form.publisher = c.publisher;
		// first_publish_year is the work's: exactly the first-published year.
		if (c.year) form.first_published = String(c.year);
		if (c.language) form.language = primaryLanguage(c.language) ?? c.language;
		if (c.isbn.length > 0 && !form.isbn.trim()) form.isbn = c.isbn[0];
		candidates = [];
		message = t('ol.applied');
		// The work's description, when ours is empty.
		if (c.work && !form.description.trim()) {
			try {
				const res = await fetch(`/api/openlibrary/work?key=${encodeURIComponent(c.work)}`);
				if (res.ok) {
					const { description }: { description: string | null } = await res.json();
					if (description && !form.description.trim()) {
						form.description = description;
						message = t('ol.appliedSummary');
					}
				}
			} catch {
				// The description is a bonus.
			}
		}
	}

	async function useCover(c: Candidate) {
		if (!c.cover_id) return;
		coverBusy = c.cover_id;
		message = '';
		try {
			const res = await fetch(`/api/books/${bookId}/cover/openlibrary`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ cover_id: c.cover_id })
			});
			if (res.ok) {
				message = t('ol.coverUsed');
				oncover();
			} else {
				message = t('ol.coverFailed');
			}
		} catch {
			message = t('common.network');
		} finally {
			coverBusy = null;
		}
	}
</script>

<fieldset>
	<legend>Open Library</legend>
	<div class="head">
		<button type="button" class="ghost" onclick={search} disabled={loading}>
			<Search size={13} />
			{loading ? t('libris.searching') : t('ol.search')}
		</button>
		<span class="hint">{t('ol.hint')}</span>
	</div>
	{#if found > candidates.length && candidates.length > 0}
		<p class="hint">{t('libris.records', { count: found, shown: candidates.length })}</p>
	{/if}
	{#if candidates.length > 0}
		<ul>
			{#each candidates as c, i (i)}
				<li>
					{#if c.cover_id}
						<img src={`/api/openlibrary/cover/${c.cover_id}?size=S`} alt="" loading="lazy" />
					{:else}
						<span class="nocover"></span>
					{/if}
					<button type="button" class="candidate" onclick={() => apply(c)}>
						<span class="c-title">{c.title ?? '—'}</span>
						<span class="c-meta">{[c.author, c.publisher, c.year, c.isbn[0]].filter(Boolean).join(' · ')}</span>
					</button>
					{#if c.cover_id}
						<button
							type="button"
							class="ghost use-cover"
							disabled={coverBusy !== null}
							title={t('ol.useCover')}
							onclick={() => useCover(c)}
						>
							<ImageDown size={13} />
							{coverBusy === c.cover_id ? t('edit.saving') : t('ol.useCover')}
						</button>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
	{#if message}<p class="msg">{message}</p>{/if}
	<p class="credit">{t('ol.credit')}</p>
</fieldset>

<style>
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
	.head {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
	}
	.hint {
		font-size: 0.75rem;
		color: var(--muted);
	}
	ul {
		list-style: none;
		margin: 0.75rem 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	li {
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}
	img,
	.nocover {
		width: 2.2rem;
		height: 3.3rem;
		object-fit: cover;
		border-radius: 2px 4px 4px 2px;
		border: 1px solid var(--border);
		flex-shrink: 0;
		background: var(--card);
	}
	.candidate {
		flex: 1;
		min-width: 0;
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
	.use-cover {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.78rem;
	}
	.msg {
		margin: 0.6rem 0 0;
		font-size: 0.85rem;
		color: var(--accent);
	}
	.credit {
		margin: 0.6rem 0 0;
		font-size: 0.72rem;
		color: var(--muted);
	}
	@media (max-width: 30rem) {
		li {
			flex-wrap: wrap;
		}
		.use-cover {
			margin-left: 2.8rem;
		}
	}
</style>
