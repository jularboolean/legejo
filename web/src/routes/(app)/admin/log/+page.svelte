<script lang="ts">
	import { goto } from '$app/navigation';
	import { ArrowLeft } from '@lucide/svelte';
	import { getLocale, t, type MessageKey } from '#lib/i18n';
	import { LOG_GROUPS, logActor, logText, type LogEntry, type LogPage } from '#lib/logText';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	let extra = $state<LogEntry[]>([]);
	let next = $state<number | null>(null);
	let loading = $state(false);
	let errorMsg = $state('');

	// A new filter (new data) starts the list over.
	$effect(() => {
		extra = [];
		next = data.page.next;
	});

	const entries = $derived([...data.page.entries, ...extra]);

	async function more() {
		if (next == null) return;
		loading = true;
		errorMsg = '';
		try {
			const params = new URLSearchParams({ before: String(next) });
			if (data.action) params.set('action', data.action);
			const res = await fetch(`/api/admin/log?${params}`);
			if (!res.ok) {
				errorMsg = t('edit.saveFailed');
				return;
			}
			const page: LogPage = await res.json();
			extra = [...extra, ...page.entries];
			next = page.next;
		} catch {
			errorMsg = t('common.network');
		} finally {
			loading = false;
		}
	}

	function filter(action: string) {
		goto(action ? `/admin/log?action=${action}` : '/admin/log');
	}

	const locale = $derived(getLocale() === 'en' ? 'en-GB' : getLocale());
	const fmtTime = $derived(new Intl.DateTimeFormat(locale, { timeStyle: 'short' }));
	const fmtDay = $derived(new Intl.DateTimeFormat(locale, { dateStyle: 'full' }));

	/** The local calendar day, for grouping (YYYY-MM-DD). */
	function day(at: string): string {
		return new Date(at).toLocaleDateString('sv-SE');
	}
</script>

<a class="back" href="/admin"><ArrowLeft size={13} /> {t('admin.heading')}</a>
<h1>{t('log.heading')}</h1>
<p class="intro">{t('log.intro')}</p>

<div class="filters" role="group" aria-label={t('log.filter')}>
	<button type="button" class:active={data.action === ''} onclick={() => filter('')}>{t('log.group.all')}</button>
	{#each LOG_GROUPS as g (g)}
		<button type="button" class:active={data.action === g} onclick={() => filter(g)}>{t(`log.group.${g}`)}</button>
	{/each}
</div>

{#if entries.length === 0}
	<p class="empty">{t('log.empty')}</p>
{:else}
	<ol class="log">
		{#each entries as e, i (e.id)}
			{#if i === 0 || day(entries[i - 1].at) !== day(e.at)}
				<li class="day">{fmtDay.format(new Date(e.at))}</li>
			{/if}
			<li class="entry">
				<time datetime={e.at}>{fmtTime.format(new Date(e.at))}</time>
				<span class="text">
					<strong>{logActor(e)}</strong>
					{logText(e)}
				</span>
				<span class="tag">{t(`log.group.${e.action.split('.')[0]}` as MessageKey)}</span>
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
		margin-bottom: 1rem;
		color: var(--muted);
	}
	h1 {
		font-size: 1.4rem;
		margin: 0 0 0.4rem;
	}
	.intro {
		margin: 0 0 1.25rem;
		color: var(--muted);
		font-size: 0.88rem;
		max-width: 40rem;
	}
	.filters {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		margin-bottom: 1.25rem;
	}
	.filters button {
		background: none;
		color: var(--muted);
		border: 1px solid var(--border);
		border-radius: 99px;
		padding: 0.2rem 0.75rem;
		font-size: 0.8rem;
		font-weight: 500;
	}
	.filters button.active {
		color: var(--fg);
		border-color: var(--accent);
		background: var(--card);
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
