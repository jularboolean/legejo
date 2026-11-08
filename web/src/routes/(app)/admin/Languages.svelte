<script lang="ts">
	import { onMount } from 'svelte';
	import { Download, LoaderCircle, Upload } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { formatBytes } from '#lib/audio';
	import { getLocale, t } from '#lib/i18n';
	import { languageInSentence } from '#lib/library/vocab';
	import {
		importPack,
		installPack,
		listPacks,
		MAX_PACK_ZIP_BYTES,
		PackError,
		removePack,
		type AvailablePack,
		type LanguagePack,
		type LanguagePacks
	} from '#lib/admin/languages';

	const CATALOGUE = 'https://legejo.se/languages';

	let list = $state<LanguagePacks | null>(null);
	let loadFailed = $state(false);
	let file = $state<File | null>(null);
	let input = $state<HTMLInputElement | null>(null);
	let importing = $state(false);
	let importError = $state('');
	let imported = $state<LanguagePack | null>(null);
	let removing = $state<LanguagePack | null>(null);
	let removeError = $state('');
	/** The language being installed or updated from the catalogue. */
	let installing = $state<string | null>(null);
	let installError = $state<{ code: string; message: string } | null>(null);
	/** What the last install did: installed, or updated (from another version). */
	let installed = $state<{ pack: LanguagePack; updated: boolean } | null>(null);

	const n = (count: number) => count.toLocaleString(getLocale());
	const catalogue = $derived(list?.catalogue || CATALOGUE);
	const offers = $derived(new Map((list?.available ?? []).map((a) => [a.code, a])));
	/** The catalogue's packs not installed yet: the suggested ones first. */
	const notInstalled = $derived.by(() => {
		if (!list) return [];
		const rest = list.available.filter((a) => a.installed_version === null);
		const rank = (a: AvailablePack) => {
			const i = list!.suggested.indexOf(a.code);
			return i === -1 ? Infinity : i;
		};
		return rest.sort((a, b) => rank(a) - rank(b) || a.english.localeCompare(b.english, getLocale()));
	});

	const books = (count: number) => (count === 1 ? t('langAdmin.oneBook') : t('langAdmin.books', { count: n(count) }));

	async function load() {
		try {
			list = await listPacks();
			loadFailed = false;
		} catch {
			loadFailed = true;
		}
	}

	onMount(load);

	function why(e: unknown): string {
		if (e instanceof PackError) {
			if (e.status === 413) return t('langAdmin.tooLarge');
			if (e.message) return e.message;
			return t('edit.saveFailed');
		}
		return t('common.network');
	}

	function clearNotes() {
		importError = '';
		imported = null;
		installError = null;
		installed = null;
		removeError = '';
	}

	async function install(code: string) {
		if (installing) return;
		clearNotes();
		const updated = list?.packs.some((p) => p.code === code) ?? false;
		installing = code;
		try {
			installed = { pack: await installPack(code), updated };
			await load();
		} catch (err) {
			installError = { code, message: why(err) };
		} finally {
			installing = null;
		}
	}

	async function upload(e: SubmitEvent) {
		e.preventDefault();
		if (!file) return;
		clearNotes();
		if (file.size > MAX_PACK_ZIP_BYTES) {
			importError = t('langAdmin.tooLarge');
			return;
		}
		importing = true;
		try {
			imported = await importPack(file);
			file = null;
			if (input) input.value = '';
			await load();
		} catch (err) {
			importError = why(err);
		} finally {
			importing = false;
		}
	}

	async function remove() {
		const pack = removing;
		removing = null;
		if (!pack) return;
		clearNotes();
		try {
			await removePack(pack.code);
			await load();
		} catch (err) {
			removeError = why(err);
		}
	}
</script>

<section>
	<h2>{t('langAdmin.heading')}</h2>
	<p class="hint">
		{t('langAdmin.intro')}
		<a href={catalogue} target="_blank" rel="noreferrer">{t('langAdmin.catalogue')}</a>
	</p>

	{#if loadFailed}
		<p class="error">{t('langAdmin.loadFailed')}</p>
	{:else if list}
		<h3>{t('langAdmin.packs')}</h3>
		<ul class="packs">
			<li>
				<div class="who">
					<div class="name-row">
						<span class="name">Esperanto</span>
						<span class="code">eo</span>
					</div>
					<div class="meta">
						<span>{t('langAdmin.esperantoBuiltIn')}</span>
						<span>{books(list.esperanto.books)}</span>
					</div>
				</div>
			</li>
			{#each list.packs as pack (pack.code)}
				{@const offer = offers.get(pack.code)}
				<li>
					<div class="who">
						<div class="name-row">
							<span class="name">{pack.name}</span>
							<span class="code">{pack.code}</span>
						</div>
						<div class="meta">
							<span>{t('langAdmin.version', { version: pack.version })}</span>
							<span>{t('langAdmin.lemmas', { count: n(pack.lemmas) })}</span>
							<span>{books(pack.books)}</span>
						</div>
						{#if pack.attribution || pack.license}
							<div class="meta">
								<span>{[pack.attribution, pack.license].filter(Boolean).join(' · ')}</span>
							</div>
						{/if}
						{#if installError?.code === pack.code}
							<p class="error">{t('langAdmin.installFailed', { error: installError.message })}</p>
						{/if}
					</div>
					<div class="actions">
						{#if offer?.update}
							<button type="button" class="small" disabled={installing !== null} onclick={() => install(pack.code)}>
								{#if installing === pack.code}
									<span class="spin"><LoaderCircle size={13} /></span>
									{t('langAdmin.updating')}
								{:else}
									{t('langAdmin.update', { version: offer.version })}
								{/if}
							</button>
						{/if}
						<button type="button" class="linkish danger" disabled={installing === pack.code} onclick={() => (removing = pack)}>
							{t('langAdmin.remove')}
						</button>
					</div>
				</li>
			{/each}
		</ul>
		{#if removeError}<p class="error">{removeError}</p>{/if}
		{#if installed}
			<p class="ok">
				{installed.updated
					? t('langAdmin.updateDone', { name: installed.pack.name, version: installed.pack.version })
					: t('langAdmin.installDone', { name: installed.pack.name })}
			</p>
		{/if}

		<h3>{t('langAdmin.available')}</h3>
		{#if list.catalogue_url === null}
			<p class="hint">{t('langAdmin.catalogueOff')}</p>
		{:else if list.catalogue_error}
			<p class="error">{t('langAdmin.catalogueFailed', { error: list.catalogue_error })}</p>
		{:else if notInstalled.length === 0}
			<p class="hint">{t('langAdmin.availableNone')}</p>
		{:else}
			<ul class="packs">
				{#each notInstalled as offer (offer.code)}
					{@const suggested = list.suggested.includes(offer.code)}
					<li class:suggested>
						<div class="who">
							<div class="name-row">
								<span class="name">{offer.name}</span>
								{#if offer.english && offer.english !== offer.name}<span class="english">{offer.english}</span>{/if}
								<span class="code">{offer.code}</span>
							</div>
							{#if suggested}
								<div class="note">
									{offer.books === 1
										? t('langAdmin.suggestedOne', { language: languageInSentence(offer.code, getLocale()) })
										: t('langAdmin.suggested', {
												count: n(offer.books),
												language: languageInSentence(offer.code, getLocale())
											})}
								</div>
							{/if}
							<div class="meta">
								<span>{t('langAdmin.lemmas', { count: n(offer.lemmas) })}</span>
								{#if offer.bytes > 0}<span>{formatBytes(offer.bytes)}</span>{/if}
								{#if !suggested}<span>{books(offer.books)}</span>{/if}
							</div>
							{#if installError?.code === offer.code}
								<p class="error">{t('langAdmin.installFailed', { error: installError.message })}</p>
							{/if}
						</div>
						<div class="actions">
							<button type="button" class="small" disabled={installing !== null} onclick={() => install(offer.code)}>
								{#if installing === offer.code}
									<span class="spin"><LoaderCircle size={13} /></span>
									{t('langAdmin.installing')}
								{:else}
									<Download size={13} />
									{t('langAdmin.install')}
								{/if}
							</button>
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	{/if}

	<form class="import" onsubmit={upload}>
		<h3>{t('langAdmin.importHeading')}</h3>
		<p class="hint">{t('langAdmin.importHint')}</p>
		<div class="fields">
			<input
				bind:this={input}
				type="file"
				accept=".zip,application/zip"
				aria-label={t('langAdmin.file')}
				disabled={importing}
				onchange={(e) => {
					file = e.currentTarget.files?.[0] ?? null;
					importError = '';
					imported = null;
				}}
			/>
			<button disabled={importing || !file}>
				<Upload size={13} />
				{importing ? t('langAdmin.importing') : t('langAdmin.import')}
			</button>
		</div>
		{#if importError}<p class="error">{t('langAdmin.importFailed', { error: importError })}</p>{/if}
		{#if imported}<p class="ok">{t('langAdmin.importDone', { name: imported.name })}</p>{/if}
	</form>
</section>

<ConfirmDialog
	open={removing !== null}
	title={t('langAdmin.removeTitle', { name: removing?.name ?? '' })}
	message={t('langAdmin.removeConfirm', { name: removing?.name ?? '' })}
	confirmLabel={t('langAdmin.remove')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={remove}
	oncancel={() => (removing = null)}
/>

<style>
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
	h3 {
		margin: 1.1rem 0 0.5rem;
		font-size: 1rem;
	}
	.hint {
		margin: 0.4rem 0 0;
		font-size: 0.8rem;
		color: var(--muted);
	}
	.packs {
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
		padding: 0.65rem 1rem;
	}
	li + li {
		border-top: 1px solid var(--border);
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
	.name {
		font-weight: 600;
	}
	.code {
		font-size: 0.75rem;
		color: var(--muted);
	}
	.english {
		font-size: 0.85rem;
		color: var(--muted);
	}
	.note {
		font-size: 0.8rem;
		color: var(--accent);
	}
	.packs + h3,
	.packs + .error + h3,
	.ok + h3 {
		margin-top: 1.4rem;
	}
	.actions {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}
	button.small {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.8rem;
		padding: 0.3rem 0.7rem;
		white-space: nowrap;
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
	}
	.who .error {
		font-size: 0.8rem;
	}
	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: 0.2rem 0.9rem;
		font-size: 0.78rem;
		color: var(--muted);
	}
	.meta span {
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.linkish {
		flex-shrink: 0;
		background: none;
		border: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		font-size: 0.8rem;
		cursor: pointer;
	}
	.linkish:hover {
		text-decoration: underline;
		filter: none;
	}
	.linkish.danger {
		color: var(--danger);
	}
	.import {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.import h3 {
		margin-bottom: 0;
	}
	.import .hint {
		margin: 0;
	}
	.fields {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
		align-items: center;
	}
	.fields input {
		flex: 1 1 12rem;
		width: auto;
	}
	.fields button {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
	}
	.ok {
		color: var(--accent);
		font-size: 0.85rem;
		margin: 0.5rem 0 0;
	}
	.error {
		margin: 0;
	}
</style>
