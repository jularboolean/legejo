<script lang="ts">
	// The private links to a shelf, for its owner: one per person the shelf is
	// given to, each an address that opens the shelf without a login, in
	// another Legejo or in a reading app, until it is taken back.
	import { Check, Copy, Link, X } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { getLocale, t } from '#lib/i18n';
	import type { ShelfLink } from '#lib/types';

	let { shelfId }: { shelfId: number } = $props();

	let links = $state<ShelfLink[]>([]);
	let name = $state('');
	let days = $state('');
	let busy = $state(false);
	let errorMsg = $state('');
	let copied = $state<number | null>(null);
	let removing = $state<ShelfLink | null>(null);

	const fmt = $derived(new Intl.DateTimeFormat(getLocale() === 'en' ? 'en-GB' : getLocale(), { dateStyle: 'medium' }));
	const address = (link: ShelfLink) => `${location.origin}/api/opds/shared/${link.token}`;
	const expired = (link: ShelfLink) => link.expires_at !== null && new Date(link.expires_at).getTime() <= Date.now();

	async function load() {
		const res = await fetch(`/api/shelves/${shelfId}/links`);
		if (res.ok) links = await res.json();
	}
	$effect(() => {
		load();
	});

	async function create(e: SubmitEvent) {
		e.preventDefault();
		if (!name.trim() || busy) return;
		busy = true;
		errorMsg = '';
		try {
			const res = await fetch(`/api/shelves/${shelfId}/links`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ name, days: days ? Number(days) : null })
			});
			if (res.ok) {
				const link: ShelfLink = await res.json();
				links = [...links, link];
				name = '';
				days = '';
				copy(link);
			} else {
				const code = (await res.json().catch(() => null))?.error;
				errorMsg = code === 'too many links' ? t('shelfLinks.tooMany') : t('shelfLinks.failed');
			}
		} catch {
			errorMsg = t('common.network');
		}
		busy = false;
	}

	async function copy(link: ShelfLink) {
		try {
			await navigator.clipboard.writeText(address(link));
			copied = link.id;
			setTimeout(() => (copied = copied === link.id ? null : copied), 2000);
		} catch {
			// No clipboard here: the address is on the page to be marked and copied.
		}
	}

	async function remove() {
		const link = removing;
		removing = null;
		if (!link) return;
		const res = await fetch(`/api/shelves/${shelfId}/links/${link.id}`, { method: 'DELETE' }).catch(() => null);
		if (res?.ok) links = links.filter((l) => l.id !== link.id);
	}
</script>

<section class="links">
	<h2><Link size={15} strokeWidth={1.75} /> {t('shelfLinks.heading')}</h2>
	<p class="hint">{t('shelfLinks.intro')}</p>

	{#if links.length > 0}
		<ul>
			{#each links as link (link.id)}
				<li class:expired={expired(link)}>
					<div class="who">
						<span class="name">{link.name}</span>
						<span class="meta">
							{#if expired(link)}
								{t('shelfLinks.expired')}
							{:else}
								{link.last_used_at ? t('shelfLinks.used', { date: fmt.format(new Date(link.last_used_at)) }) : t('shelfLinks.neverUsed')}{#if link.expires_at}&ensp;·&ensp;{t('shelfLinks.expires', { date: fmt.format(new Date(link.expires_at)) })}{/if}
							{/if}
						</span>
						<input class="address" type="text" readonly value={address(link)} aria-label={t('shelfLinks.copy')} onfocus={(e) => e.currentTarget.select()} />
					</div>
					<button type="button" class="ghost" onclick={() => copy(link)}>
						{#if copied === link.id}<Check size={13} /> {t('kobo.copied')}{:else}<Copy size={13} /> {t('shelfLinks.copy')}{/if}
					</button>
					<button type="button" class="ghost icon" aria-label={t('shelfLinks.remove')} title={t('shelfLinks.remove')} onclick={() => (removing = link)}>
						<X size={15} />
					</button>
				</li>
			{/each}
		</ul>
	{/if}

	<form onsubmit={create}>
		<label>
			<span>{t('shelfLinks.name')}</span>
			<input type="text" bind:value={name} maxlength="80" placeholder={t('shelfLinks.namePlaceholder')} />
		</label>
		<label>
			<span>{t('shelfLinks.lasts')}</span>
			<select bind:value={days}>
				<option value="">{t('shelfLinks.forever')}</option>
				<option value="7">{t('shelfLinks.days', { count: 7 })}</option>
				<option value="30">{t('shelfLinks.days', { count: 30 })}</option>
				<option value="365">{t('shelfLinks.days', { count: 365 })}</option>
			</select>
		</label>
		<button type="submit" disabled={busy || !name.trim()}>{t('shelfLinks.create')}</button>
	</form>
	{#if errorMsg}<p class="error" role="alert">{errorMsg}</p>{/if}
</section>

<ConfirmDialog
	open={removing !== null}
	title={t('shelfLinks.remove')}
	message={removing ? t('shelfLinks.removeConfirm', { name: removing.name }) : ''}
	confirmLabel={t('shelfLinks.remove')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={remove}
	oncancel={() => (removing = null)}
/>

<style>
	.links {
		max-width: 38rem;
		margin-top: 2.5rem;
		padding-top: 1.25rem;
		border-top: 1px solid var(--border);
	}
	h2 {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 1.05rem;
		margin: 0 0 0.4rem;
	}
	.hint {
		color: var(--muted);
		font-size: 0.88rem;
		margin: 0 0 1rem;
	}
	ul {
		list-style: none;
		margin: 0 0 1.25rem;
		padding: 0;
	}
	li {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		padding: 0.65rem 0;
		border-bottom: 1px solid var(--border);
	}
	li.expired .name,
	li.expired .address {
		color: var(--muted);
		text-decoration: line-through;
	}
	.who {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}
	.name {
		font-weight: 600;
	}
	.meta {
		font-size: 0.82rem;
		color: var(--muted);
	}
	.address {
		width: 100%;
		margin-top: 0.25rem;
		font-family: ui-monospace, 'SF Mono', Menlo, monospace;
		font-size: 0.75rem;
		color: var(--muted);
		background: none;
		border: none;
		padding: 0;
		text-overflow: ellipsis;
	}
	li button {
		flex: none;
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.82rem;
		white-space: nowrap;
	}
	form {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: 0.75rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.85rem;
	}
	label:first-child {
		flex: 1;
		min-width: 12rem;
	}
	.error {
		color: var(--danger);
		font-size: 0.88rem;
	}
</style>
