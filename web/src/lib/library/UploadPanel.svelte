<script lang="ts" module>
	export type UploadItem = {
		/** Stable within one batch. */
		key: number;
		name: string;
		size: number;
		state: 'waiting' | 'sending' | 'checking' | 'added' | 'duplicate' | 'error';
		/** Bytes sent so far. */
		sent: number;
		message?: string;
		bookId?: number;
		/** Codes of what the server repaired in the file. */
		fixed: string[];
		/** How many things are still wrong with it. */
		issues: number;
	};
</script>

<script lang="ts">
	import { Check, CircleAlert, Copy, LoaderCircle, X } from '@lucide/svelte';
	import { fixedText } from '#lib/health';
	import { t } from '#lib/i18n';

	let { items, onclose }: { items: UploadItem[]; onclose: () => void } = $props();

	const total = $derived(items.reduce((n, i) => n + i.size, 0));
	const sent = $derived(items.reduce((n, i) => n + (i.state === 'waiting' || i.state === 'sending' ? i.sent : i.size), 0));
	const percent = $derived(total > 0 ? Math.round((sent / total) * 100) : 0);
	const finished = $derived(items.filter((i) => !['waiting', 'sending', 'checking'].includes(i.state)).length);
	const busy = $derived(finished < items.length);
	const added = $derived(items.filter((i) => i.state === 'added').length);
</script>

<section class="panel" aria-label={t('upload.heading')}>
	<header>
		<span class="summary" role="status">
			{#if busy}
				{t('upload.progress', { done: Math.min(finished + 1, items.length), total: items.length })}
			{:else}
				{added === 1 ? t('upload.doneOne') : t('upload.done', { count: added })}
			{/if}
		</span>
		{#if !busy}
			<button type="button" class="close" aria-label={t('dup.dismiss')} title={t('dup.dismiss')} onclick={onclose}>
				<X size={14} />
			</button>
		{/if}
	</header>
	{#if busy}
		<div
			class="bar"
			role="progressbar"
			aria-valuemin="0"
			aria-valuemax="100"
			aria-valuenow={percent}
			aria-label={t('upload.heading')}
		>
			<div class="fill" style:width={`${percent}%`}></div>
		</div>
	{/if}
	<ul>
		{#each items as item (item.key)}
			<li class={item.state}>
				<span class="icon" aria-hidden="true">
					{#if item.state === 'added'}
						<Check size={14} />
					{:else if item.state === 'error'}
						<CircleAlert size={14} />
					{:else if item.state === 'duplicate'}
						<Copy size={13} />
					{:else if item.state !== 'waiting'}
						<span class="spin"><LoaderCircle size={14} /></span>
					{/if}
				</span>
				<span class="name">
					{#if item.bookId}<a href={`/books/${item.bookId}`}>{item.name}</a>{:else}{item.name}{/if}
				</span>
				<span class="status">
					{#if item.state === 'waiting'}
						{t('upload.waiting')}
					{:else if item.state === 'sending'}
						{Math.round((item.sent / Math.max(item.size, 1)) * 100)} %
					{:else if item.state === 'checking'}
						{t('upload.checking')}
					{:else if item.state === 'added'}
						{t('upload.added')}
					{:else if item.state === 'duplicate'}
						{t('upload.duplicate')}
					{:else}
						{t('upload.failed')}
					{/if}
				</span>
				{#if item.message}
					<span class="detail">{item.message}</span>
				{/if}
				{#if item.fixed.length > 0}
					<span class="detail">{t('upload.repaired', { list: item.fixed.map(fixedText).join(', ') })}</span>
				{/if}
				{#if item.issues > 0 && item.bookId}
					<span class="detail">
						<a href={`/books/${item.bookId}`}>
							{item.issues === 1 ? t('upload.issue') : t('upload.issues', { count: item.issues })}
						</a>
					</span>
				{/if}
			</li>
		{/each}
	</ul>
</section>

<style>
	.panel {
		background: var(--card);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 0.7rem 0.9rem;
		margin-bottom: 1rem;
		font-size: 0.88rem;
	}
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
	}
	.summary {
		font-weight: 500;
	}
	.close {
		padding: 0.2rem;
		border: none;
		background: none;
		color: var(--muted);
	}
	.bar {
		height: 4px;
		margin-top: 0.5rem;
		border-radius: 2px;
		background: var(--border);
		overflow: hidden;
	}
	.fill {
		height: 100%;
		background: var(--accent);
		transition: width 0.2s linear;
	}
	ul {
		list-style: none;
		margin: 0.6rem 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		max-height: 14rem;
		overflow-y: auto;
	}
	li {
		display: grid;
		grid-template-columns: 1.1rem 1fr auto;
		column-gap: 0.4rem;
		align-items: center;
	}
	.icon {
		display: inline-flex;
		color: var(--muted);
	}
	li.added .icon {
		color: var(--accent);
	}
	li.error .icon {
		color: var(--danger);
	}
	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	li.waiting .name {
		color: var(--muted);
	}
	.status {
		color: var(--muted);
		font-size: 0.8rem;
		font-variant-numeric: tabular-nums;
	}
	.detail {
		grid-column: 2 / -1;
		color: var(--muted);
		font-size: 0.8rem;
	}
	li.error .detail {
		color: var(--danger);
	}
	.spin {
		display: inline-flex;
		animation: spin 1s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.spin {
			animation: none;
		}
		.fill {
			transition: none;
		}
	}
</style>
