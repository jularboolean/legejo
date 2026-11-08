<script lang="ts">
	import { onMount } from 'svelte';
	import { Upload } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { getLocale, t } from '#lib/i18n';
	import {
		importPack,
		listPacks,
		MAX_PACK_ZIP_BYTES,
		PackError,
		removePack,
		type LanguagePack
	} from '#lib/admin/languages';

	const CATALOGUE = 'https://legejo.se/languages';

	let packs = $state<LanguagePack[] | null>(null);
	let catalogue = $state(CATALOGUE);
	let loadFailed = $state(false);
	let file = $state<File | null>(null);
	let input = $state<HTMLInputElement | null>(null);
	let importing = $state(false);
	let importError = $state('');
	let imported = $state<LanguagePack | null>(null);
	let removing = $state<LanguagePack | null>(null);
	let removeError = $state('');

	const n = (count: number) => count.toLocaleString(getLocale());

	async function load() {
		try {
			const list = await listPacks();
			packs = list.packs;
			catalogue = list.catalogue || CATALOGUE;
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

	async function upload(e: SubmitEvent) {
		e.preventDefault();
		if (!file) return;
		importError = '';
		imported = null;
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
		removeError = '';
		imported = null;
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
	{:else if packs}
		<h3>{t('langAdmin.packs')}</h3>
		<ul class="packs">
			{#each packs as pack (pack.code)}
				<li>
					<div class="who">
						<div class="name-row">
							<span class="name">{pack.name}</span>
							<span class="code">{pack.code}</span>
							{#if pack.built_in}
								<span class="badge">{t('langAdmin.kind.builtIn')}</span>
							{:else if pack.replaces_built_in}
								<span class="badge warn">{t('langAdmin.kind.replaces')}</span>
							{:else}
								<span class="badge">{t('langAdmin.kind.imported')}</span>
							{/if}
						</div>
						<div class="meta">
							<span>{t('langAdmin.version', { version: pack.version })}</span>
							<span>{t('langAdmin.lemmas', { count: n(pack.lemmas) })}</span>
							<span>{pack.books === 1 ? t('langAdmin.oneBook') : t('langAdmin.books', { count: n(pack.books) })}</span>
						</div>
						{#if pack.attribution || pack.license}
							<div class="meta">
								<span>{[pack.attribution, pack.license].filter(Boolean).join(' · ')}</span>
							</div>
						{/if}
					</div>
					{#if !pack.built_in}
						<button type="button" class="linkish danger" onclick={() => (removing = pack)}>{t('langAdmin.remove')}</button>
					{/if}
				</li>
			{/each}
		</ul>
		{#if removeError}<p class="error">{removeError}</p>{/if}
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
	message={removing?.replaces_built_in
		? t('langAdmin.removeConfirmBuiltIn', { name: removing?.name ?? '' })
		: t('langAdmin.removeConfirm', { name: removing?.name ?? '' })}
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
	.badge {
		display: inline-flex;
		align-items: center;
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
		margin: 0;
	}
	.error {
		margin: 0;
	}
</style>
