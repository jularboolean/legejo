<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { ArrowLeft, Check, ChevronLeft, ChevronRight, Copy, ImagePlus, ImageOff, Trash2 } from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import { SLUG_RE } from '#lib/fed';
	import { t } from '#lib/i18n';
	import { reasonText } from '#lib/license';
	import { defaultShelfCoverIndex, shelfCovers } from '#lib/shelfCovers';
	import type { BlockingBook } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	let form = $state({
		name: data.shelf.name,
		description: data.shelf.description ?? '',
		visibility: data.shelf.visibility
	});
	let hasCover = $state(data.shelf.has_cover);
	// Bumped after each upload so the preview bypasses the browser cache.
	let coverVersion = $state(0);
	let fileInput: HTMLInputElement;
	// What the preview shows: one of the offered covers, or null for the
	// shelf's own uploaded image. The arrows only browse; a pick is stored
	// when the form is saved.
	const startPick = () => (hasCover ? null : defaultShelfCoverIndex(data.shelf.id));
	let stored = $state<number | null>(data.shelf.has_cover ? null : defaultShelfCoverIndex(data.shelf.id));
	let picked = $state<number | null>(data.shelf.has_cover ? null : defaultShelfCoverIndex(data.shelf.id));
	// The stops the arrows cycle through: the own image first, when there is one.
	const stops = $derived<(number | null)[]>([...(hasCover ? [null] : []), ...shelfCovers.map((_, i) => i)]);
	const position = $derived(stops.indexOf(picked));

	function browse(step: number) {
		picked = stops[(position + step + stops.length) % stops.length];
	}
	let saving = $state(false);
	let errorMsg = $state('');
	let deleteOpen = $state(false);

	// Federation. The slug is chosen once, the first time the
	// shelf federates; after that the handle is fixed and only shown.
	const fedAvailable = $derived(data.fed.available);
	const slugFixed = $derived(data.shelf.ap_slug != null);
	let slug = $state(data.shelf.ap_slug ?? data.shelf.suggested_slug);
	const slugValid = $derived(SLUG_RE.test(slug));
	const handlePreview = $derived(`@${slug}@${data.fed.host ?? '…'}`);
	const fixedHandle = $derived(
		data.shelf.handle ?? `@${data.shelf.ap_slug}@${data.fed.host ?? '…'}`
	);
	let blocking = $state<BlockingBook[]>([]);
	let handleCopied = $state(false);

	async function copyHandle() {
		try {
			await navigator.clipboard.writeText(fixedHandle);
			handleCopied = true;
			setTimeout(() => (handleCopied = false), 2000);
		} catch {
			// Clipboard refused (insecure origin); the field is selectable anyway.
		}
	}

	const SAVE_ERRORS: Record<string, Parameters<typeof t>[0]> = {
		'name must not be empty': 'shelfEdit.nameRequired',
		'federation is off': 'fed.error.off',
		'invalid handle': 'fed.error.invalidHandle',
		'handle taken': 'fed.error.handleTaken',
		'handle cannot change': 'fed.error.handleCannotChange',
		'books not federable': 'fed.error.shelfNotFederable'
	};

	async function removeShelf() {
		deleteOpen = false;
		const res = await fetch(`/api/shelves/${data.shelf.id}`, { method: 'DELETE' });
		if (res.ok) {
			await invalidateAll();
			goto('/');
		} else {
			errorMsg = t('edit.saveFailed');
		}
	}

	async function sendCover(file: Blob, name: string): Promise<boolean> {
		errorMsg = '';
		const body = new FormData();
		body.append('cover', file, name);
		try {
			const res = await fetch(`/api/shelves/${data.shelf.id}/cover`, { method: 'POST', body });
			if (!res.ok) {
				const err = await res.json().catch(() => null);
				errorMsg = err?.error ?? t('shelfEdit.coverFailed');
				return false;
			}
			hasCover = true;
			coverVersion++;
			return true;
		} catch {
			errorMsg = t('common.network');
			return false;
		}
	}

	async function uploadCover(files: FileList | null) {
		if (!files || files.length === 0) return;
		if (await sendCover(files[0], files[0].name)) stored = picked = null;
		fileInput.value = '';
	}

	// An offered cover becomes the shelf's own: stored like an upload, so it
	// also shows on public and federated views.
	async function storePick(index: number): Promise<boolean> {
		try {
			const image = await (await fetch(shelfCovers[index])).blob();
			return await sendCover(image, `cover${index + 1}.jpg`);
		} catch {
			errorMsg = t('common.network');
			return false;
		}
	}

	async function removeCover() {
		errorMsg = '';
		const res = await fetch(`/api/shelves/${data.shelf.id}/cover`, { method: 'DELETE' });
		if (res.ok) {
			hasCover = false;
			stored = picked = startPick();
		}
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		errorMsg = '';
		blocking = [];
		const goingFederated = form.visibility === 'federated' && !slugFixed;
		if (goingFederated && !slugValid) {
			errorMsg = t('fed.error.invalidHandle');
			return;
		}
		saving = true;
		try {
			if (picked !== null && picked !== stored && !(await storePick(picked))) return;
			const res = await fetch(`/api/shelves/${data.shelf.id}`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					name: form.name,
					description: form.description || null,
					visibility: form.visibility,
					...(goingFederated ? { ap_slug: slug } : {})
				})
			});
			if (res.ok) {
				await invalidateAll();
				goto(`/shelves/${data.shelf.id}`);
			} else {
				const body = await res.json().catch(() => null);
				const key = SAVE_ERRORS[body?.error];
				if (key) {
					errorMsg = t(key);
					if (body.error === 'books not federable') blocking = body.blocking ?? [];
				} else {
					errorMsg = res.status === 409 ? t('edit.shelfExists') : t('edit.saveFailed');
				}
			}
		} catch {
			errorMsg = t('common.network');
		} finally {
			saving = false;
		}
	}
</script>

<a class="back" href={`/shelves/${data.shelf.id}`}><ArrowLeft size={13} /> {data.shelf.name}</a>

<h1>{t('shelfEdit.heading')}</h1>

<form onsubmit={save}>
	<div class="cover-section">
		<span class="cover-label">{t('shelfEdit.cover')}</span>
		<div class="cover-frame">
			<img
				class="cover-preview"
				src={picked !== null
					? shelfCovers[picked]
					: `/api/shelves/${data.shelf.id}/cover?v=${coverVersion}`}
				alt=""
			/>
			<button type="button" class="cover-nav prev" aria-label={t('shelfEdit.coverPrev')} onclick={() => browse(-1)}>
				<ChevronLeft size={18} />
			</button>
			<button type="button" class="cover-nav next" aria-label={t('shelfEdit.coverNext')} onclick={() => browse(1)}>
				<ChevronRight size={18} />
			</button>
			<span class="cover-count" aria-live="polite">{position + 1} / {stops.length}</span>
		</div>
		<span class="hint cover-hint">{t('shelfEdit.pickCover')}</span>
		<div class="cover-actions">
			<button type="button" class="ghost" onclick={() => fileInput.click()}>
				<ImagePlus size={13} />
				{t('shelfEdit.chooseCover')}
			</button>
			{#if hasCover}
				<button type="button" class="ghost" onclick={removeCover}>
					<ImageOff size={13} />
					{t('shelfEdit.removeCover')}
				</button>
			{/if}
		</div>
		<input
			type="file"
			accept="image/*"
			hidden
			bind:this={fileInput}
			onchange={(e) => uploadCover(e.currentTarget.files)}
		/>
	</div>

	<label>
		{t('shelfEdit.name')}
		<input bind:value={form.name} required />
	</label>
	<label>
		{t('shelfEdit.description')}
		<textarea bind:value={form.description} rows="6"></textarea>
		<span class="hint">{t('edit.descriptionHint')}</span>
	</label>
	<fieldset class="visibility">
		<legend>{t('shelfEdit.visibility')}</legend>
		<label class="choice">
			<input type="radio" bind:group={form.visibility} value="private" />
			<span>
				{t('shelfEdit.visibility.private')}
				<span class="hint">{t('shelfEdit.visibility.privateHint')}</span>
			</span>
		</label>
		<label class="choice">
			<input type="radio" bind:group={form.visibility} value="instance" />
			<span>
				{t('shelfEdit.visibility.instance')}
				<span class="hint">{t('shelfEdit.visibility.instanceHint')}</span>
			</span>
		</label>
		<label class="choice" class:disabled={!fedAvailable && data.shelf.visibility !== 'federated'}>
			<input
				type="radio"
				bind:group={form.visibility}
				value="federated"
				disabled={!fedAvailable && data.shelf.visibility !== 'federated'}
			/>
			<span>
				{t('shelfEdit.visibility.federated')}
				<span class="hint">
					{fedAvailable
						? t('shelfEdit.visibility.federatedHint')
						: t('shelfEdit.visibility.federatedOff')}
				</span>
			</span>
		</label>

		{#if form.visibility === 'federated'}
			<div class="fed">
				{#if slugFixed}
					<span class="fed-label">{t('shelfEdit.handle')}</span>
					<div class="handle-row">
						<input readonly value={fixedHandle} onfocus={(e) => e.currentTarget.select()} />
						<button type="button" class="ghost" onclick={copyHandle}>
							{#if handleCopied}<Check size={13} /> {t('kobo.copied')}{:else}<Copy size={13} /> {t('shelfEdit.copyHandle')}{/if}
						</button>
					</div>
					<span class="hint">
						{data.shelf.followers === 1
							? t('shelfEdit.follower')
							: t('shelfEdit.followers', { count: data.shelf.followers })}
						· {t('shelfEdit.handleFixed')}
					</span>
				{:else}
					<label>
						{t('shelfEdit.slug')}
						<input
							bind:value={slug}
							autocapitalize="off"
							autocomplete="off"
							spellcheck="false"
							aria-invalid={!slugValid}
						/>
						<span class="hint">
							{t('shelfEdit.handlePreview')} <code>{handlePreview}</code>
						</span>
						{#if !slugValid}
							<span class="hint bad">{t('fed.error.invalidHandle')}</span>
						{:else}
							<span class="hint">{t('shelfEdit.slugHint')}</span>
						{/if}
					</label>
				{/if}
			</div>
		{/if}
	</fieldset>

	{#if errorMsg}<p class="error">{errorMsg}</p>{/if}
	{#if blocking.length > 0}
		<ul class="blocking">
			{#each blocking as b (b.id)}
				<li><a href={`/books/${b.id}/edit`}>{b.title}</a>: {reasonText(b.reason)}</li>
			{/each}
		</ul>
	{/if}

	<div class="actions">
		<button type="submit" disabled={saving}>{saving ? t('edit.saving') : t('edit.save')}</button>
		<a class="cancel" href={`/shelves/${data.shelf.id}`}>{t('common.cancel')}</a>
	</div>
</form>

<div class="danger-zone">
	<button type="button" class="ghost danger" onclick={() => (deleteOpen = true)}>
		<Trash2 size={13} />
		{t('shelf.delete')}
	</button>
	<span class="hint">{t('shelfEdit.deleteHint')}</span>
</div>

<ConfirmDialog
	open={deleteOpen}
	title={t('shelf.delete')}
	message={t('shelf.deleteConfirm', { name: data.shelf.name })}
	confirmLabel={t('shelf.delete')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={removeShelf}
	oncancel={() => (deleteOpen = false)}
/>

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
		margin: 0 0 1.25rem;
	}
	form {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		max-width: 38rem;
	}
	.cover-section {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.cover-label {
		font-size: 0.9rem;
		color: var(--muted);
	}
	.cover-preview {
		width: 100%;
		height: 12rem;
		object-fit: cover;
		border-radius: 8px;
		border: 1px solid var(--border);
	}
	.cover-frame {
		position: relative;
		display: flex;
	}
	.cover-nav {
		position: absolute;
		top: 50%;
		transform: translateY(-50%);
		padding: 0.35rem;
		border-radius: 99px;
		background: rgba(0, 0, 0, 0.45);
		color: #fff;
		border: none;
	}
	.cover-nav:hover {
		filter: none;
		background: rgba(0, 0, 0, 0.7);
	}
	.cover-nav.prev {
		left: 0.5rem;
	}
	.cover-nav.next {
		right: 0.5rem;
	}
	.cover-count {
		position: absolute;
		right: 0.6rem;
		bottom: 0.5rem;
		padding: 0.05rem 0.45rem;
		border-radius: 99px;
		background: rgba(0, 0, 0, 0.45);
		color: #fff;
		font-size: 0.72rem;
		font-variant-numeric: tabular-nums;
	}
	.cover-section .cover-hint {
		margin-top: 0;
	}
	.cover-actions {
		display: flex;
		gap: 0.5rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.9rem;
		color: var(--muted);
	}
	fieldset.visibility {
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 0.75rem 1rem 1rem;
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
	}
	legend {
		font-size: 0.9rem;
		color: var(--muted);
		padding: 0 0.3rem;
	}
	label.choice {
		flex-direction: row;
		align-items: start;
		gap: 0.55rem;
		color: var(--fg);
		cursor: pointer;
	}
	label.choice > span {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}
	label.choice.disabled {
		cursor: default;
		opacity: 0.6;
	}
	.fed {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		margin-left: 1.4rem;
		padding-left: 0.8rem;
		border-left: 2px solid var(--border);
	}
	.fed-label {
		font-size: 0.9rem;
		color: var(--muted);
	}
	.handle-row {
		display: flex;
		gap: 0.5rem;
	}
	.handle-row input,
	.fed code {
		font-family: ui-monospace, 'SF Mono', monospace;
		font-size: 0.82rem;
		overflow-wrap: anywhere;
	}
	.handle-row button {
		flex-shrink: 0;
	}
	.fed .hint {
		margin-top: 0;
	}
	.hint.bad {
		color: var(--danger);
	}
	.blocking {
		margin: -0.5rem 0 0;
		padding-left: 1.2rem;
		font-size: 0.85rem;
		color: var(--danger);
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
	}
	input[type='radio'] {
		width: auto;
		margin: 0.2rem 0 0;
		accent-color: var(--accent);
	}
	.hint {
		font-size: 0.75rem;
		color: var(--muted);
		margin-top: -0.75rem;
	}
	label .hint {
		margin-top: 0;
	}
	textarea {
		font: inherit;
		background: var(--card);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 0.5rem 0.75rem;
		resize: vertical;
	}
	textarea:focus {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}
	.actions {
		display: flex;
		align-items: center;
		gap: 1rem;
	}
	.cancel {
		color: var(--muted);
	}
	.danger-zone {
		margin-top: 2.5rem;
		padding-top: 1.25rem;
		border-top: 1px solid var(--border);
		max-width: 38rem;
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}
	.danger {
		color: var(--danger);
		border-color: var(--danger);
		flex-shrink: 0;
	}
	.danger-zone .hint {
		font-size: 0.8rem;
		color: var(--muted);
	}
</style>
