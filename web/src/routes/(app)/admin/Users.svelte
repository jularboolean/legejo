<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { BookOpen, Library, Mail, ShieldCheck } from '@lucide/svelte';
	import Avatar from '#lib/Avatar.svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { t } from '#lib/i18n';
	import type { AdminUserRow } from '#lib/types';
	import Invite from './Invite.svelte';

	let { users, me, mailConfigured }: { users: AdminUserRow[]; me: number; mailConfigured: boolean } = $props();

	let errorMsg = $state('');
	let busyId = $state<number | null>(null);
	// Admin rights are confirmed before they change, both ways.
	let adminChange = $state<{ user: AdminUserRow; to: boolean } | null>(null);

	async function setAdmin() {
		if (!adminChange) return;
		const { user, to } = adminChange;
		adminChange = null;
		errorMsg = '';
		busyId = user.id;
		try {
			const res = await fetch(`/api/admin/users/${user.id}/admin`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ is_admin: to })
			});
			if (!res.ok) errorMsg = t('edit.saveFailed');
			await invalidateAll();
		} catch {
			errorMsg = t('common.network');
		} finally {
			busyId = null;
		}
	}

	// Pending invites: a fresh link, or withdraw (removes the waiting account).
	let inviteMsg = $state<{ id: number; text: string; link?: string } | null>(null);
	let withdrawing = $state<{ id: number; username: string } | null>(null);

	async function renewInvite(id: number) {
		inviteMsg = null;
		const res = await fetch(`/api/admin/invites/${id}/renew`, { method: 'POST' });
		const body = await res.json().catch(() => null);
		if (!res.ok) {
			inviteMsg = { id, text: t('edit.saveFailed') };
			return;
		}
		inviteMsg = {
			id,
			text: body.mailed
				? t('invite.mailed', { username: body.username })
				: t('invite.copyLink', { username: body.username }),
			link: body.link
		};
	}

	async function withdrawInvite() {
		if (!withdrawing) return;
		const id = withdrawing.id;
		withdrawing = null;
		const res = await fetch(`/api/admin/invites/${id}`, { method: 'DELETE' });
		if (res.ok) await invalidateAll();
	}
</script>

<section>
	<h2>{t('admin.users')} <span class="count">{users.length}</span></h2>
	<Invite {mailConfigured} />
	{#if errorMsg}<p class="error">{errorMsg}</p>{/if}

	<ul class="users">
		{#each users as user (user.id)}
			<li class:pending={user.invited}>
				<Avatar userId={user.id} hasAvatar={user.has_avatar} size={36} alt="" />
				<div class="who">
					<div class="name-row">
						<span class="username">{user.username}</span>
						{#if user.id === me}<span class="you">{t('adminUsers.you')}</span>{/if}
						{#if user.is_admin}
							<span class="badge"><ShieldCheck size={11} /> {t('admin.adminBadge')}</span>
						{/if}
						{#if user.invited}
							<span class="badge warn">{t('invite.pending')}</span>
						{:else if !user.verified}
							<span class="badge warn">{t('admin.unverified')}</span>
						{/if}
					</div>
					<div class="meta">
						{#if user.email}<span><Mail size={12} /> {user.email}</span>{/if}
						<span><BookOpen size={12} /> {t('adminUsers.books', { count: user.book_count })}</span>
						<span><Library size={12} /> {t('adminUsers.shelves', { count: user.shelf_count })}</span>
						<span>{t('adminUsers.since', { date: user.created_at.slice(0, 10) })}</span>
					</div>
					{#if user.invited}
						<div class="invite-actions">
							<button type="button" class="linkish" onclick={() => renewInvite(user.id)}>{t('invite.renew')}</button>
							·
							<button
								type="button"
								class="linkish danger"
								onclick={() => (withdrawing = { id: user.id, username: user.username })}
							>
								{t('invite.withdraw')}
							</button>
						</div>
						{#if inviteMsg?.id === user.id}
							<div class="invite-msg">
								{inviteMsg.text}
								{#if inviteMsg.link}<input readonly value={inviteMsg.link} onfocus={(e) => e.currentTarget.select()} />{/if}
							</div>
						{/if}
					{/if}
				</div>
				<label class="admin-toggle" title={user.id === me ? t('adminUsers.notSelf') : ''}>
					<input
						type="checkbox"
						checked={user.is_admin}
						disabled={user.id === me || busyId === user.id}
						onclick={(e) => {
							e.preventDefault();
							adminChange = { user, to: !user.is_admin };
						}}
					/>
					{t('admin.adminBadge')}
				</label>
			</li>
		{/each}
	</ul>
</section>

<ConfirmDialog
	open={adminChange !== null}
	title={adminChange?.to ? t('adminUsers.grantTitle') : t('adminUsers.revokeTitle')}
	message={adminChange?.to
		? t('adminUsers.grant', { username: adminChange?.user.username ?? '' })
		: t('adminUsers.revoke', { username: adminChange?.user.username ?? '' })}
	confirmLabel={adminChange?.to ? t('adminUsers.grantTitle') : t('adminUsers.revokeTitle')}
	cancelLabel={t('common.cancel')}
	danger={adminChange?.to ?? false}
	onconfirm={setAdmin}
	oncancel={() => (adminChange = null)}
/>

<ConfirmDialog
	open={withdrawing !== null}
	title={t('invite.withdraw')}
	message={t('invite.withdrawConfirm', { username: withdrawing?.username ?? '' })}
	confirmLabel={t('invite.withdraw')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={withdrawInvite}
	oncancel={() => (withdrawing = null)}
/>

<style>
	section {
		max-width: 40rem;
		margin-bottom: 2.5rem;
	}
	.count {
		font-size: 0.8rem;
		font-weight: 400;
		color: var(--muted);
		margin-left: 0.3rem;
	}
	.users {
		list-style: none;
		margin: 0;
		padding: 0;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--card);
		overflow: hidden;
	}
	li {
		display: flex;
		align-items: center;
		gap: 0.85rem;
		padding: 0.75rem 1rem;
	}
	li + li {
		border-top: 1px solid var(--border);
	}
	li.pending {
		background: color-mix(in srgb, var(--card) 85%, var(--bg));
	}
	li > :global(img) {
		flex-shrink: 0;
		border-radius: 50%;
	}
	.who {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
	}
	.name-row {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.4rem;
	}
	.username {
		font-weight: 600;
	}
	.you {
		font-size: 0.75rem;
		color: var(--muted);
	}
	.badge {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		font-size: 0.7rem;
		color: var(--accent);
		border: 1px solid var(--accent);
		border-radius: 99px;
		padding: 0 0.45rem;
	}
	.badge.warn {
		color: var(--danger);
		border-color: var(--danger);
	}
	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: 0.2rem 0.9rem;
		font-size: 0.78rem;
		color: var(--muted);
	}
	.meta span {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.admin-toggle {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.82rem;
		color: var(--muted);
		cursor: pointer;
	}
	.invite-actions {
		font-size: 0.8rem;
	}
	.linkish {
		background: none;
		border: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		cursor: pointer;
	}
	.linkish:hover {
		text-decoration: underline;
		filter: none;
	}
	.linkish.danger {
		color: var(--danger);
	}
	.invite-msg {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.8rem;
	}
	.invite-msg input {
		font-size: 0.75rem;
	}
	@media (max-width: 30rem) {
		li {
			flex-wrap: wrap;
		}
		.admin-toggle {
			margin-left: calc(36px + 0.85rem);
		}
	}
</style>
