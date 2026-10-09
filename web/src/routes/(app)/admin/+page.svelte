<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import Tabs from '#lib/Tabs.svelte';
	import { t } from '#lib/i18n';
	import Federation from './Federation.svelte';
	import Users from './Users.svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	// The page in parts, the part in the address: /admin?tab=users.
	const wanted = $derived(page.url.searchParams.get('tab'));
	const tab = $derived(wanted === 'users' || (wanted === 'federation' && data.federation) ? wanted : 'settings');
	const tabs = $derived([
		{ href: '/admin', label: t('tabs.settings'), active: tab === 'settings' },
		{ href: '/admin?tab=users', label: t('admin.users'), active: tab === 'users' },
		...(data.federation
			? [{ href: '/admin?tab=federation', label: t('fedAdmin.heading'), active: tab === 'federation', dot: (data.fed.pending_instances ?? 0) > 0 }]
			: []),
		{ href: '/admin/log', label: t('log.heading'), active: false }
	]);

	let librisEnabled = $state(data.settings.libris_enabled);
	let openLibraryEnabled = $state(data.settings.openlibrary_enabled ?? false);
	let audiobooksEnabled = $state(data.settings.audiobooks_enabled ?? false);
	let catalogsEnabled = $state(data.settings.catalogs_enabled ?? false);
	let registrationEnabled = $state(data.settings.registration_enabled);
	const mailConfigured = data.settings.mail_configured;
	let saving = $state(false);
	let errorMsg = $state('');
	let testTo = $state('');
	let testMsg = $state('');
	let testError = $state('');

	async function saveSettings() {
		saving = true;
		errorMsg = '';
		try {
			const res = await fetch('/api/admin/settings', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					libris_enabled: librisEnabled,
					openlibrary_enabled: openLibraryEnabled,
					audiobooks_enabled: audiobooksEnabled,
					catalogs_enabled: catalogsEnabled,
					registration_enabled: registrationEnabled
				})
			});
			if (!res.ok) {
				errorMsg = t('edit.saveFailed');
				return false;
			}
			const settings: {
				libris_enabled: boolean;
				openlibrary_enabled: boolean;
				audiobooks_enabled: boolean;
				catalogs_enabled?: boolean;
				registration_enabled: boolean;
			} = await res.json();
			audiobooksEnabled = settings.audiobooks_enabled;
			catalogsEnabled = settings.catalogs_enabled ?? false;
			librisEnabled = settings.libris_enabled;
			openLibraryEnabled = settings.openlibrary_enabled;
			registrationEnabled = settings.registration_enabled;
			await invalidateAll();
			return true;
		} catch {
			errorMsg = t('common.network');
			return false;
		} finally {
			saving = false;
		}
	}

	async function toggleLibris(enabled: boolean) {
		const previous = librisEnabled;
		librisEnabled = enabled;
		if (!(await saveSettings())) librisEnabled = previous;
	}

	async function toggleOpenLibrary(enabled: boolean) {
		const previous = openLibraryEnabled;
		openLibraryEnabled = enabled;
		if (!(await saveSettings())) openLibraryEnabled = previous;
	}

	async function toggleAudiobooks(enabled: boolean) {
		const previous = audiobooksEnabled;
		audiobooksEnabled = enabled;
		if (!(await saveSettings())) audiobooksEnabled = previous;
	}

	async function toggleCatalogs(enabled: boolean) {
		const previous = catalogsEnabled;
		catalogsEnabled = enabled;
		if (!(await saveSettings())) catalogsEnabled = previous;
	}

	async function toggleRegistration(enabled: boolean) {
		const previous = registrationEnabled;
		registrationEnabled = enabled;
		if (!(await saveSettings())) registrationEnabled = previous;
	}

	async function sendTest() {
		testMsg = '';
		testError = '';
		try {
			const res = await fetch('/api/admin/test-mail', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ to: testTo })
			});
			if (res.ok) {
				testMsg = t('admin.testSent');
			} else {
				const body = await res.json().catch(() => null);
				testError = body?.error ?? t('edit.saveFailed');
			}
		} catch {
			testError = t('common.network');
		}
	}
</script>

<h1>{t('admin.heading')}</h1>
<Tabs {tabs} label={t('admin.heading')} />

{#if tab === 'settings'}
<section>
	<h2>{t('admin.features')}</h2>
	<label class="toggle">
		<input
			type="checkbox"
			checked={librisEnabled}
			disabled={saving}
			onchange={(e) => toggleLibris(e.currentTarget.checked)}
		/>
		{t('admin.librisToggle')}
	</label>
	<p class="hint">{t('admin.librisHint')}</p>
	<label class="toggle reg">
		<input
			type="checkbox"
			checked={openLibraryEnabled}
			disabled={saving}
			onchange={(e) => toggleOpenLibrary(e.currentTarget.checked)}
		/>
		{t('admin.openLibraryToggle')}
	</label>
	<p class="hint">{t('admin.openLibraryHint')}</p>
	<label class="toggle reg">
		<input
			type="checkbox"
			checked={audiobooksEnabled}
			disabled={saving}
			onchange={(e) => toggleAudiobooks(e.currentTarget.checked)}
		/>
		{t('admin.audiobooksToggle')}
	</label>
	<p class="hint">{t('admin.audiobooksHint')}</p>
	<label class="toggle reg">
		<input
			type="checkbox"
			checked={catalogsEnabled}
			disabled={saving}
			onchange={(e) => toggleCatalogs(e.currentTarget.checked)}
		/>
		{t('admin.catalogsToggle')}
	</label>
	<p class="hint">{t('admin.catalogsHint')}</p>
	<label class="toggle reg">
		<input
			type="checkbox"
			checked={registrationEnabled}
			disabled={saving}
			onchange={(e) => toggleRegistration(e.currentTarget.checked)}
		/>
		{t('admin.registrationToggle')}
	</label>
	<p class="hint">{t('admin.registrationHint')}</p>
	{#if errorMsg}<p class="error">{errorMsg}</p>{/if}
</section>

{#if data.settings.librarian}
	{@const ai = data.settings.librarian}
	<section>
		<h2>{t('librarian.heading')}</h2>
		<p class="hint">{t('admin.librarianHint', { model: ai.model, endpoint: ai.endpoint })}</p>
		<p class="hint">
			{t('admin.librarianUse', { questions: ai.questions, input: ai.prompt_tokens, output: ai.completion_tokens })}
		</p>
	</section>
{/if}

<section>
	<h2>{t('admin.mail')}</h2>
	<p class="hint">
		{mailConfigured ? t('admin.mailConfigured') : t('admin.mailNotConfigured')}
	</p>
	{#if mailConfigured}
		<div class="mailtest">
			<input bind:value={testTo} type="email" placeholder={t('admin.testTo')} />
			<button type="button" class="ghost" onclick={sendTest} disabled={!testTo.trim()}>
				{t('admin.testSend')}
			</button>
		</div>
		{#if testMsg}<p class="ok">{testMsg}</p>{/if}
		{#if testError}<p class="error">{testError}</p>{/if}
	{/if}
</section>

{:else if tab === 'federation' && data.federation}
	<Federation settings={data.federation} instances={data.instances} overview={data.overview} />
{:else}
	<Users users={data.users} me={data.user?.id ?? 0} {mailConfigured} />
{/if}

<style>
	h1 {
		font-size: 1.4rem;
		margin: 0 0 0.6rem;
	}
	section {
		max-width: 34rem;
		margin-bottom: 2.5rem;
	}
	h2 {
		font-size: 1.1rem;
		margin: 0 0 0.75rem;
		padding-bottom: 0.4rem;
		border-bottom: 1px solid var(--border);
	}
	.toggle {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.95rem;
	}
	.toggle.reg {
		margin-top: 1rem;
	}
	.mailtest {
		margin-top: 0.75rem;
		display: flex;
		gap: 0.5rem;
	}
	.mailtest button {
		flex-shrink: 0;
	}
	.ok {
		color: var(--accent);
		font-size: 0.85rem;
		margin: 0;
	}
	.hint {
		margin: 0.4rem 0 0;
		font-size: 0.8rem;
		color: var(--muted);
	}
</style>
