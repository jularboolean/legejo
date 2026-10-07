<script lang="ts">
	import { X } from '@lucide/svelte';
	import Avatar from '#lib/Avatar.svelte';
	import { t } from '#lib/i18n';
	import type { ShelfMember } from '#lib/types';

	/** The users something is shared with; the search only proposes users to add. */
	let { members = $bindable() }: { members: ShelfMember[] } = $props();

	let query = $state('');
	let hits = $state<ShelfMember[]>([]);
	let searched = $state(false);
	let run = 0;

	async function search() {
		const q = query.trim();
		const mine = ++run;
		if (q.length < 2) {
			hits = [];
			searched = false;
			return;
		}
		try {
			const res = await fetch(`/api/users/search?q=${encodeURIComponent(q)}`);
			const found: ShelfMember[] = res.ok ? await res.json() : [];
			// A slower, older answer must not replace a newer one.
			if (mine !== run) return;
			hits = found.filter((u) => !members.some((m) => m.id === u.id));
			searched = true;
		} catch {
			if (mine === run) hits = [];
		}
	}

	function add(user: ShelfMember) {
		members = [...members, user].sort((a, b) => a.username.localeCompare(b.username));
		query = '';
		hits = [];
		searched = false;
	}
</script>

<div class="picker">
	<span class="label">{t('shelfEdit.members.label')}</span>
	{#if members.length === 0}
		<span class="hint">{t('shelfEdit.members.none')}</span>
	{:else}
		<ul class="members">
			{#each members as member (member.id)}
				<li>
					<Avatar userId={member.id} hasAvatar={member.has_avatar} size={20} alt="" />
					<span class="name">{member.username}</span>
					<button
						type="button"
						class="remove"
						aria-label={t('shelfEdit.members.remove', { name: member.username })}
						title={t('shelfEdit.members.remove', { name: member.username })}
						onclick={() => (members = members.filter((m) => m.id !== member.id))}
					>
						<X size={13} />
					</button>
				</li>
			{/each}
		</ul>
	{/if}
	<input
		type="search"
		bind:value={query}
		oninput={search}
		placeholder={t('shelfEdit.members.search')}
		aria-label={t('shelfEdit.members.search')}
		autocapitalize="off"
		autocomplete="off"
		spellcheck="false"
	/>
	{#if hits.length > 0}
		<ul class="hits">
			{#each hits as hit (hit.id)}
				<li>
					<button type="button" class="hit" onclick={() => add(hit)}>
						<Avatar userId={hit.id} hasAvatar={hit.has_avatar} size={20} alt="" />
						{hit.username}
					</button>
				</li>
			{/each}
		</ul>
	{:else if searched}
		<span class="hint" aria-live="polite">{t('shelfEdit.members.noMatch')}</span>
	{/if}
</div>

<style>
	.picker {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		margin-left: 1.4rem;
		padding-left: 0.8rem;
		border-left: 2px solid var(--border);
	}
	.label {
		font-size: 0.9rem;
		color: var(--muted);
	}
	.hint {
		font-size: 0.8rem;
		color: var(--muted);
	}
	.members,
	.hits {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.members li {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.9rem;
		color: var(--fg);
	}
	.name {
		flex: 1;
		overflow-wrap: anywhere;
	}
	.remove {
		padding: 0.25rem;
		border: none;
		background: none;
		color: var(--muted);
	}
	.remove:hover {
		color: var(--danger);
	}
	.hits {
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 0.25rem;
		background: var(--card);
	}
	.hit {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		width: 100%;
		padding: 0.3rem 0.4rem;
		border: none;
		background: none;
		color: var(--fg);
		font-size: 0.9rem;
		text-align: left;
	}
	.hit:hover,
	.hit:focus-visible {
		background: var(--border);
		filter: none;
	}
</style>
