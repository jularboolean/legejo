<script lang="ts">
	// Shelves that someone has sent a private link to, from this instance or
	// another: the list of them, and the field a new link is pasted into.
	import { goto, invalidateAll } from '$app/navigation';
	import { CloudOff, Link, Unlink, X } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { plainSnippet } from '#lib/library';
	import { defaultShelfCover } from '#lib/shelfCovers';
	import { t } from '#lib/i18n';
	import type { LinkedShelf } from '#lib/types';

	let { shelves: loaded }: { shelves: LinkedShelf[] } = $props();

	// The list comes at once with what the server knew, and the server then
	// asks the shelves it has not asked for a while. Asking the list once
	// more, a little later, shows what came of that.
	let fresh = $state<LinkedShelf[] | null>(null);
	const shelves = $derived(fresh ?? loaded);
	$effect(() => {
		void loaded;
		fresh = null;
		const timer = setTimeout(async () => {
			const res = await fetch('/api/linked-shelves').catch(() => null);
			if (res?.ok) fresh = await res.json();
		}, 4000);
		return () => clearTimeout(timer);
	});

	let url = $state('');
	let busy = $state(false);
	let errorMsg = $state('');
	let removing = $state<LinkedShelf | null>(null);

	const ERRORS: Record<string, Parameters<typeof t>[0]> = {
		'not a shelf link': 'linked.error.notLink',
		missing: 'linked.error.missing',
		refused: 'linked.error.missing',
		'not a catalog': 'linked.error.notLink',
		'address not allowed': 'linked.error.address',
		'too many shelves': 'linked.error.tooMany'
	};

	async function follow(e: SubmitEvent) {
		e.preventDefault();
		if (!url.trim() || busy) return;
		busy = true;
		errorMsg = '';
		try {
			const res = await fetch('/api/linked-shelves', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ url })
			});
			const body = await res.json().catch(() => null);
			if (res.ok || res.status === 409) {
				url = '';
				await invalidateAll();
				await goto(`/linked/${body.id}`);
			} else {
				errorMsg = t(ERRORS[body?.error] ?? 'linked.error.unreachable');
			}
		} catch {
			errorMsg = t('common.network');
		}
		busy = false;
	}

	async function unfollow() {
		const shelf = removing;
		removing = null;
		if (!shelf) return;
		await fetch(`/api/linked-shelves/${shelf.id}`, { method: 'DELETE' }).catch(() => null);
		await invalidateAll();
	}
</script>

<section class="linked">
	<h2><Link size={16} strokeWidth={1.75} /> {t('linked.heading')}</h2>

	{#if shelves.length > 0}
		<ul>
			{#each shelves as shelf (shelf.id)}
				<li>
					<a class="row" href={`/linked/${shelf.id}`}>
						<img class="thumb" src={defaultShelfCover(shelf.id)} alt="" loading="lazy" />
						<span class="info">
							<span class="name">{shelf.title}</span>
							<span class="from">
								{t('linked.from', { host: shelf.host })}
								{#if shelf.state === 'unreachable'}
									<span class="state"><CloudOff size={12} strokeWidth={1.75} /> {t('linked.state.unreachable')}</span>
								{:else if shelf.state === 'gone'}
									<span class="state"><Unlink size={12} strokeWidth={1.75} /> {t('linked.state.gone')}</span>
								{/if}
							</span>
							{#if shelf.description}<span class="description">{plainSnippet(shelf.description)}</span>{/if}
						</span>
					</a>
					<button type="button" class="ghost icon" aria-label={t('linked.unfollow')} title={t('linked.unfollow')} onclick={() => (removing = shelf)}>
						<X size={15} />
					</button>
				</li>
			{/each}
		</ul>
	{/if}

	<form onsubmit={follow}>
		<label for="linked-url">{t('linked.intro')}</label>
		<div class="field">
			<input id="linked-url" type="url" bind:value={url} placeholder={t('linked.placeholder')} autocomplete="off" spellcheck="false" />
			<button type="submit" disabled={busy || !url.trim()}>{busy ? t('linked.following') : t('linked.follow')}</button>
		</div>
		{#if errorMsg}<p class="error" role="alert">{errorMsg}</p>{/if}
	</form>
</section>

<ConfirmDialog
	open={removing !== null}
	title={t('linked.unfollow')}
	message={removing ? t('linked.unfollowConfirm', { name: removing.title }) : ''}
	confirmLabel={t('linked.unfollow')}
	cancelLabel={t('common.cancel')}
	onconfirm={unfollow}
	oncancel={() => (removing = null)}
/>

<style>
	.linked {
		margin-top: 2.75rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--border);
		max-width: 44rem;
	}
	h2 {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		font-size: 1.1rem;
		margin: 0 0 0.9rem;
	}
	ul {
		list-style: none;
		margin: 0 0 1.25rem;
		padding: 0;
	}
	li {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		border-bottom: 1px solid var(--border);
	}
	.row {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 0.9rem;
		padding: 0.7rem 0;
		color: var(--fg);
		text-decoration: none;
	}
	.thumb {
		flex: none;
		width: 3.4rem;
		aspect-ratio: 4 / 3;
		object-fit: cover;
		border-radius: 6px;
	}
	.info {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
	}
	.name {
		font-weight: 600;
	}
	.from,
	.description {
		font-size: 0.85rem;
		color: var(--muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.from {
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}
	.state {
		flex: none;
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.05rem 0.45rem;
		border: 1px solid var(--border);
		border-radius: 999px;
		font-size: 0.75rem;
		color: var(--danger);
	}
	label {
		display: block;
		font-size: 0.9rem;
		color: var(--muted);
		margin-bottom: 0.45rem;
	}
	.field {
		display: flex;
		gap: 0.5rem;
	}
	.field input {
		flex: 1;
		min-width: 0;
	}
	.error {
		color: var(--danger);
		font-size: 0.88rem;
		margin: 0.5rem 0 0;
	}
</style>
