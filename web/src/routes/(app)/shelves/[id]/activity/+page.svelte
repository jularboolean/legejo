<script lang="ts">
	import { ArrowLeft } from '@lucide/svelte';
	import { getLocale, t } from '#lib/i18n';
	import { eventText, fromElsewhere, type ShelfEvent, type ShelfEventPage } from '#lib/shelfEvents';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const shelf = $derived(data.shelf);
	let extra = $state<ShelfEvent[]>([]);
	let next = $state<number | null>(null);
	let loading = $state(false);
	let errorMsg = $state('');

	// Another shelf starts the list over.
	$effect(() => {
		extra = [];
		next = data.page.next;
	});

	const entries = $derived([...data.page.entries, ...extra]);
	const shared = $derived(shelf.visibility !== 'private');

	async function more() {
		if (next == null) return;
		loading = true;
		errorMsg = '';
		try {
			const res = await fetch(`/api/shelves/${shelf.id}/events?before=${next}`);
			if (!res.ok) {
				errorMsg = t('edit.saveFailed');
				return;
			}
			const page: ShelfEventPage = await res.json();
			extra = [...extra, ...page.entries];
			next = page.next;
		} catch {
			errorMsg = t('common.network');
		} finally {
			loading = false;
		}
	}

	const locale = $derived(getLocale() === 'en' ? 'en-GB' : getLocale());
	const fmtTime = $derived(new Intl.DateTimeFormat(locale, { timeStyle: 'short' }));
	const fmtDay = $derived(new Intl.DateTimeFormat(locale, { dateStyle: 'full' }));

	/** The local calendar day, for grouping (YYYY-MM-DD). */
	function day(at: string): string {
		return new Date(at).toLocaleDateString('sv-SE');
	}
</script>

<a class="back" href={`/shelves/${shelf.id}`}><ArrowLeft size={15} /> {shelf.name}</a>
<h1>{t('shelfLog.heading')}</h1>
<p class="intro">{t('shelfLog.intro')}</p>

{#if entries.length === 0}
	<p class="empty">{shared ? t('shelfLog.empty') : t('shelfLog.emptyPrivate')}</p>
{:else}
	<ol class="log">
		{#each entries as e, i (e.id)}
			{#if i === 0 || day(entries[i - 1].at) !== day(e.at)}
				<li class="day">{fmtDay.format(new Date(e.at))}</li>
			{/if}
			<li class="entry">
				<time datetime={e.at}>{fmtTime.format(new Date(e.at))}</time>
				<span class="text">{eventText(e)}</span>
				{#if fromElsewhere(e)}<span class="tag">{t(e.kind === 'link_download' ? 'shelfLog.tagLink' : 'shelfLog.tagFed')}</span>{/if}
			</li>
		{/each}
	</ol>
	{#if next != null}
		<button type="button" class="ghost more" disabled={loading} onclick={more}>
			{loading ? t('log.loading') : t('log.more')}
		</button>
	{/if}
{/if}
{#if errorMsg}<p class="error">{errorMsg}</p>{/if}

<style>
	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		color: var(--muted);
		font-size: 0.9rem;
		margin-bottom: 0.75rem;
	}
	h1 {
		font-size: 1.4rem;
		margin: 0 0 0.6rem;
	}
	.intro {
		margin: 0 0 1.25rem;
		color: var(--muted);
		font-size: 0.88rem;
		max-width: 40rem;
	}
	.log {
		list-style: none;
		margin: 0;
		padding: 0;
		max-width: 44rem;
	}
	.day {
		font-size: 0.72rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--muted);
		margin: 1rem 0 0.3rem;
	}
	.day:first-child {
		margin-top: 0;
	}
	.entry {
		display: grid;
		grid-template-columns: 3.2rem minmax(0, 1fr) auto;
		align-items: baseline;
		gap: 0.75rem;
		padding: 0.45rem 0;
		border-top: 1px solid var(--border);
		font-size: 0.9rem;
	}
	time {
		color: var(--muted);
		font-size: 0.8rem;
		font-variant-numeric: tabular-nums;
	}
	.text {
		overflow-wrap: anywhere;
	}
	.tag {
		font-size: 0.7rem;
		color: var(--muted);
		border: 1px solid var(--border);
		border-radius: 99px;
		padding: 0 0.45rem;
		white-space: nowrap;
	}
	.empty {
		color: var(--muted);
		font-style: italic;
	}
	.more {
		margin-top: 1rem;
	}
	@media (max-width: 30rem) {
		.entry {
			grid-template-columns: 3rem minmax(0, 1fr);
		}
		.tag {
			display: none;
		}
	}
</style>
