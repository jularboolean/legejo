<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import portrait from '#lib/assets/librarian.svg';
	import { t, type MessageKey } from '#lib/i18n';
	import { loadView, saveView, type ViewMode } from '#lib/library';
	import BookGrid from '#lib/library/BookGrid.svelte';
	import BookRows from '#lib/library/BookRows.svelte';
	import SharedHits from '#lib/library/SharedHits.svelte';
	import ViewSwitch from '#lib/library/ViewSwitch.svelte';
	import type { Book, PublicHit } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	type Answer = { books: Book[]; shared: PublicHit[]; looked_through: number; all: boolean; prompt_tokens: number; completion_tokens: number };

	let question = $state('');
	/** The question the answer on the page belongs to. */
	let asked = $state('');
	let answer = $state<Answer | null>(null);
	/** Which of the librarian's ways of handing books over goes with this answer. */
	let phrase = $state(1);
	let asking = $state(false);
	let errorMsg = $state('');
	let view = $state<ViewMode>(loadView());

	// A question costs the operator a little, so nothing is asked while it
	// is being written, and the last answer is kept for the way back from a
	// book rather than asked for again.
	const KEPT = 'legejo.librarian.last';

	async function ask(e?: SubmitEvent) {
		e?.preventDefault();
		const q = question.trim();
		if (!q || asking) return;
		asking = true;
		errorMsg = '';
		try {
			const res = await fetch('/api/librarian', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ question: q })
			});
			if (res.ok) {
				answer = await res.json();
				asked = q;
				phrase = 1 + Math.floor(Math.random() * 4);
				try {
					sessionStorage.setItem(KEPT, JSON.stringify({ asked, answer, phrase }));
				} catch {
					// No storage: the answer just does not survive leaving the page.
				}
			} else {
				errorMsg = res.status === 429 ? t('librarian.tooMany') : t('librarian.failed');
			}
		} catch {
			errorMsg = t('common.network');
		} finally {
			asking = false;
		}
	}

	onMount(() => {
		// Sent here with a question, from the search page: ask it once, and
		// take it out of the address so that a reload does not ask again.
		const q = page.url.searchParams.get('q')?.trim();
		if (q) {
			question = q;
			goto('/librarian', { replace: true });
			ask();
			return;
		}
		try {
			const kept = JSON.parse(sessionStorage.getItem(KEPT) ?? 'null');
			if (kept?.answer?.books && kept.answer.shared) {
				answer = kept.answer;
				asked = question = kept.asked ?? '';
				if ([1, 2, 3, 4].includes(kept.phrase)) phrase = kept.phrase;
			}
		} catch {
			// Nothing kept, or something else's data.
		}
	});

	function setView(next: ViewMode) {
		view = next;
		saveView(next);
	}
</script>

<div class="head">
	<img class="portrait" src={portrait} alt="" />
	<div>
		<h1>{t('librarian.heading')}</h1>
		<p class="intro">{t('librarian.intro')}</p>
	</div>
</div>

<form class="ask" onsubmit={ask}>
	<input
		type="text"
		bind:value={question}
		placeholder={t('librarian.placeholder')}
		aria-label={t('librarian.heading')}
		maxlength="300"
		autocomplete="off"
	/>
	<button type="submit" disabled={asking || question.trim() === ''}>
		{asking ? t('librarian.asking') : t('librarian.ask')}
	</button>
</form>
<p class="privacy">{t('librarian.privacy')}</p>

{#if errorMsg}<p class="error">{errorMsg}</p>{/if}

{#if answer && !asking}
	<div class="result">
		<p class="said">
			{#if answer.books.length === 0 && answer.shared.length === 0}
				{t('librarian.none')}
			{:else}
				{t(`librarian.found.${phrase}` as MessageKey)}
			{/if}
			<span class="looked">
				{answer.all
					? t('librarian.lookedAll', { count: answer.looked_through })
					: t('librarian.lookedSome', { count: answer.looked_through })}
				{#if data.user?.is_admin}
					· {t('librarian.usage', { input: answer.prompt_tokens, output: answer.completion_tokens })}
				{/if}
			</span>
		</p>
		{#if answer.books.length > 0 || answer.shared.length > 0}<ViewSwitch {view} onchange={setView} />{/if}
	</div>
	{#if answer.books.length > 0}
		{#if view === 'list'}
			<BookRows books={answer.books} showLanguage />
		{:else}
			<BookGrid books={answer.books} showLanguage />
		{/if}
	{/if}
	{#if answer.shared.length > 0}
		<h2>{t('librarian.shared')}</h2>
		<SharedHits hits={answer.shared} {view} />
	{/if}
{/if}

<style>
	.head {
		display: flex;
		align-items: center;
		gap: 1.1rem;
		margin-bottom: 1.25rem;
	}
	.portrait {
		width: 5rem;
		height: 5rem;
		flex-shrink: 0;
		border-radius: 50%;
		object-fit: cover;
		object-position: 50% 30%;
		background: var(--card);
		border: 1px solid var(--border);
		box-shadow: var(--shadow);
	}
	h1 {
		font-size: 1.4rem;
		margin: 0 0 0.25rem;
	}
	h2 {
		font-size: 1.05rem;
		margin: 2rem 0 0.75rem;
		padding-bottom: 0.35rem;
		border-bottom: 1px solid var(--border);
	}
	.intro {
		margin: 0;
		color: var(--muted);
		max-width: 36rem;
	}
	.ask {
		display: flex;
		flex-direction: row;
		gap: 0.6rem;
		max-width: 44rem;
	}
	.ask input {
		flex: 1;
		font-size: 1rem;
		padding: 0.6rem 0.85rem;
	}
	.ask button {
		flex-shrink: 0;
	}
	.privacy {
		margin: 0.5rem 0 1.75rem;
		font-size: 0.8rem;
		color: var(--muted);
		max-width: 44rem;
	}
	.result {
		display: flex;
		align-items: start;
		justify-content: space-between;
		gap: 1rem;
		margin-bottom: 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--border);
	}
	.said {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
	}
	.looked {
		font-size: 0.8rem;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	@media (max-width: 40rem) {
		.ask {
			flex-direction: column;
		}
	}
</style>
