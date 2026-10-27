<script lang="ts">
	import WhatsNew from '#lib/WhatsNew.svelte';
	import { invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import Tabs from '#lib/Tabs.svelte';
	import { getLocale, t } from '#lib/i18n';
	import { isNewer, latestRelease, type Release } from '#lib/version';
	import Connections from './Connections.svelte';
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
	let koboAnnotationsEnabled = $state(data.settings.kobo_annotations_enabled ?? false);
	let koboStoreEnabled = $state(data.settings.kobo_store_enabled ?? false);
	let dictionaryEnabled = $state(data.settings.dictionary_enabled ?? false);
	let whatsNewOpen = $state(false);
	let shelfLinksEnabled = $state(data.settings.shelf_links_enabled ?? false);
	let authorFactsEnabled = $state(data.settings.author_facts_enabled ?? false);
	let fulltextEnabled = $state(data.settings.fulltext_enabled ?? false);
	let registrationEnabled = $state(data.settings.registration_enabled);
	const mailConfigured = data.settings.mail_configured;
	let saving = $state(false);
	let errorMsg = $state('');

	// Is there a newer Legejo? Asked of GitHub from this browser, on request.
	let versionState = $state<'idle' | 'checking' | 'latest' | 'newer' | 'failed'>('idle');
	let newerRelease = $state<Release | null>(null);

	async function checkVersion() {
		versionState = 'checking';
		newerRelease = null;
		try {
			const release = await latestRelease();
			if (isNewer(release.version, __APP_VERSION__)) {
				newerRelease = release;
				versionState = 'newer';
			} else {
				versionState = 'latest';
			}
		} catch {
			versionState = 'failed';
		}
	}

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
					kobo_annotations_enabled: koboAnnotationsEnabled,
					kobo_store_enabled: koboStoreEnabled,
					dictionary_enabled: dictionaryEnabled,
					shelf_links_enabled: shelfLinksEnabled,
					author_facts_enabled: authorFactsEnabled,
					fulltext_enabled: data.settings.fulltext_env ? undefined : fulltextEnabled,
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
				kobo_annotations_enabled?: boolean;
				kobo_store_enabled?: boolean;
				dictionary_enabled?: boolean;
				shelf_links_enabled?: boolean;
				author_facts_enabled?: boolean;
				fulltext_enabled?: boolean;
				registration_enabled: boolean;
			} = await res.json();
			audiobooksEnabled = settings.audiobooks_enabled;
			catalogsEnabled = settings.catalogs_enabled ?? false;
			koboAnnotationsEnabled = settings.kobo_annotations_enabled ?? false;
			koboStoreEnabled = settings.kobo_store_enabled ?? false;
			dictionaryEnabled = settings.dictionary_enabled ?? false;
			shelfLinksEnabled = settings.shelf_links_enabled ?? false;
			authorFactsEnabled = settings.author_facts_enabled ?? false;
			fulltextEnabled = settings.fulltext_enabled ?? false;
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

	async function toggleKoboAnnotations(enabled: boolean) {
		const previous = koboAnnotationsEnabled;
		koboAnnotationsEnabled = enabled;
		if (!(await saveSettings())) koboAnnotationsEnabled = previous;
	}

	async function toggleDictionary(enabled: boolean) {
		const previous = dictionaryEnabled;
		dictionaryEnabled = enabled;
		if (!(await saveSettings())) dictionaryEnabled = previous;
	}

	async function toggleKoboStore(enabled: boolean) {
		const previous = koboStoreEnabled;
		koboStoreEnabled = enabled;
		if (!(await saveSettings())) koboStoreEnabled = previous;
	}

	async function toggleShelfLinks(enabled: boolean) {
		const previous = shelfLinksEnabled;
		shelfLinksEnabled = enabled;
		if (!(await saveSettings())) shelfLinksEnabled = previous;
	}

	async function toggleAuthorFacts(enabled: boolean) {
		const previous = authorFactsEnabled;
		authorFactsEnabled = enabled;
		if (!(await saveSettings())) authorFactsEnabled = previous;
	}

	async function toggleFulltext(enabled: boolean) {
		const previous = fulltextEnabled;
		fulltextEnabled = enabled;
		if (!(await saveSettings())) fulltextEnabled = previous;
	}

	async function toggleRegistration(enabled: boolean) {
		const previous = registrationEnabled;
		registrationEnabled = enabled;
		if (!(await saveSettings())) registrationEnabled = previous;
	}
</script>

<h1>{t('admin.heading')}</h1>
<Tabs {tabs} label={t('admin.heading')} />

{#if tab === 'settings'}
<section>
	<h2>{t('admin.group.metadata')}</h2>
	<label class="toggle">
		<input
			type="checkbox"
			checked={openLibraryEnabled}
			disabled={saving}
			onchange={(e) => toggleOpenLibrary(e.currentTarget.checked)}
		/>
		{t('admin.openLibraryToggle')}
	</label>
	<p class="hint">{t('admin.openLibraryHint')}</p>
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
	<label class="toggle">
		<input
			type="checkbox"
			checked={authorFactsEnabled}
			disabled={saving}
			onchange={(e) => toggleAuthorFacts(e.currentTarget.checked)}
		/>
		{t('admin.authorFactsToggle')}
	</label>
	<p class="hint">{t('admin.authorFactsHint')}</p>
</section>

<section>
	<h2>{t('admin.group.kobo')}</h2>
	<label class="toggle">
		<input
			type="checkbox"
			checked={koboAnnotationsEnabled}
			disabled={saving}
			onchange={(e) => toggleKoboAnnotations(e.currentTarget.checked)}
		/>
		{t('admin.koboAnnotationsToggle')}
	</label>
	<p class="hint">{t('admin.koboAnnotationsHint')}</p>
	<label class="toggle">
		<input
			type="checkbox"
			checked={koboStoreEnabled}
			disabled={saving}
			onchange={(e) => toggleKoboStore(e.currentTarget.checked)}
		/>
		{t('admin.koboStoreToggle')}
	</label>
	<p class="hint">{t('admin.koboStoreHint')}</p>
</section>

<section>
	<h2>{t('admin.group.library')}</h2>
	<label class="toggle">
		<input
			type="checkbox"
			checked={audiobooksEnabled}
			disabled={saving}
			onchange={(e) => toggleAudiobooks(e.currentTarget.checked)}
		/>
		{t('admin.audiobooksToggle')}
	</label>
	<p class="hint">{t('admin.audiobooksHint')}</p>
	<label class="toggle">
		<input
			type="checkbox"
			checked={dictionaryEnabled}
			disabled={saving}
			onchange={(e) => toggleDictionary(e.currentTarget.checked)}
		/>
		{t('admin.dictionaryToggle')}
	</label>
	<p class="hint">{t('admin.dictionaryHint')}</p>
	{#if data.settings.fulltext_env}
		<p class="fulltext">
			<strong>{t('admin.fulltextToggle')}:</strong>
			{data.settings.fulltext_enabled ? t('admin.fulltextIsOn') : t('admin.fulltextOff')}
		</p>
		<p class="hint">{t('conn.fromEnv', { name: 'LEGEJO_FULLTEXT' })}</p>
	{:else}
		<label class="toggle">
			<input type="checkbox" checked={fulltextEnabled} disabled={saving} onchange={(e) => toggleFulltext(e.currentTarget.checked)} />
			{t('admin.fulltextToggle')}
		</label>
	{/if}
	{#if data.settings.fulltext}
		{@const ft = data.settings.fulltext}
		<p class="hint">
			{t('admin.fulltextProgress', {
				books: ft.books,
				pending: ft.pending,
				size: (ft.bytes / 1_000_000).toLocaleString(getLocale(), { maximumFractionDigits: 1 })
			})}
		</p>
	{/if}
	<p class="hint">{t('admin.fulltextHint')}</p>
</section>

<section>
	<h2>{t('admin.group.sharing')}</h2>
	<label class="toggle">
		<input
			type="checkbox"
			checked={catalogsEnabled}
			disabled={saving}
			onchange={(e) => toggleCatalogs(e.currentTarget.checked)}
		/>
		{t('admin.catalogsToggle')}
	</label>
	<p class="hint">{t('admin.catalogsHint')}</p>
	<label class="toggle">
		<input
			type="checkbox"
			checked={shelfLinksEnabled}
			disabled={saving}
			onchange={(e) => toggleShelfLinks(e.currentTarget.checked)}
		/>
		{t('admin.shelfLinksToggle')}
	</label>
	<p class="hint">{t('admin.shelfLinksHint')}</p>
</section>

<section>
	<h2>{t('admin.group.accounts')}</h2>
	<label class="toggle">
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

{#if data.connections}
	<Connections connections={data.connections} usage={data.settings.librarian ?? null} />
{/if}

<section>
	<h2>{t('admin.version')}</h2>
	<p class="hint">{t('admin.versionRunning', { version: __APP_VERSION__ })}</p>
	<div class="mailtest">
		<button type="button" class="ghost" onclick={checkVersion} disabled={versionState === 'checking'}>
			{versionState === 'checking' ? t('admin.versionChecking') : t('admin.versionCheck')}
		</button>
		<button type="button" class="ghost" onclick={() => (whatsNewOpen = true)}>{t('admin.whatsNew')}</button>
	</div>
	{#if whatsNewOpen}<WhatsNew since={null} onclose={() => (whatsNewOpen = false)} />{/if}
	{#if versionState === 'latest'}
		<p class="ok">{t('admin.versionLatest')}</p>
	{:else if versionState === 'newer' && newerRelease}
		<p class="ok">
			{t('admin.versionNewer', { version: newerRelease.version })}
			<a href={newerRelease.url} target="_blank" rel="noreferrer">{t('admin.versionSeeRelease')}</a>
		</p>
	{:else if versionState === 'failed'}
		<p class="error">{t('admin.versionFailed')}</p>
	{/if}
	<p class="hint">{t('admin.versionHint')}</p>
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
	/* Each feature: its switch (or status line) and its explanation, the same
	   distance from the one before. */
	.hint + .toggle,
	.hint + .fulltext {
		margin-top: 1.1rem;
	}
	.mailtest {
		margin-top: 0.75rem;
		display: flex;
		flex-wrap: wrap;
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
	.fulltext {
		margin: 0;
		font-size: 0.95rem;
	}
	.hint {
		margin: 0.4rem 0 0;
		font-size: 0.8rem;
		color: var(--muted);
	}
</style>
