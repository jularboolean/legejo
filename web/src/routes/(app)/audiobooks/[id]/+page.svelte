<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import {
		ArrowLeft,
		Check,
		Copy,
		Headphones,
		ImagePlus,
		Layers,
		Pause,
		Pencil,
		Play,
		Podcast,
		RefreshCw,
		Tag,
		Trash2,
		Users
	} from '@lucide/svelte';
	import ConfirmDialog from '#lib/ConfirmDialog.svelte';
	import MemberPicker from '#lib/MemberPicker.svelte';
	import { formatBytes, formatLength } from '#lib/audio';
	import { t } from '#lib/i18n';
	import type { Audiobook, AudiobookDetail, AudiobookPart, ShelfMember } from '#lib/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();
	const book = $derived(data.audiobook);

	const partTitle = (part: AudiobookPart) => part.title ?? t('audio.partN', { n: part.position });
	const clock = (seconds: number) => {
		const m = Math.floor(seconds / 60);
		return seconds >= 3600
			? `${Math.floor(seconds / 3600)}:${String(m % 60).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`
			: `${m}:${String(seconds % 60).padStart(2, '0')}`;
	};

	// ---- The podcast feed ----
	let copied = $state(false);
	async function copyFeed() {
		try {
			await navigator.clipboard.writeText(book.feed_url);
			copied = true;
			setTimeout(() => (copied = false), 2000);
		} catch {
			// Clipboard refused (insecure origin); the field is selectable anyway.
		}
	}
	// Podcast apps register schemes of their own, which take the feed's
	// address without its scheme and open the app at that feed.
	const bare = $derived(book.feed_url.replace(/^https?:\/\//, ''));
	const apps = $derived([
		{ name: 'Apple Podcasts', href: `podcast://${bare}` },
		{ name: 'Pocket Casts', href: `pktc://subscribe/${bare}` },
		{ name: 'Overcast', href: `overcast://x-callback-url/add?url=${encodeURIComponent(book.feed_url)}` },
		{ name: 'Castro', href: `castro://subscribe/${bare}` }
	]);
	let confirmKey = $state(false);
	async function newKey() {
		confirmKey = false;
		const res = await fetch(`/api/audiobooks/${book.id}/feed-key`, { method: 'POST' });
		if (res.ok) await invalidateAll();
	}

	// ---- Editing ----
	let editing = $state(false);
	let form = $state({ title: '', author: '', narrator: '', language: '', description: '', category: '', tags: '' });
	let visibility = $state<Audiobook['visibility']>('private');
	let members = $state<ShelfMember[]>([]);
	let errorMsg = $state('');
	function startEdit() {
		form = {
			title: book.title,
			author: book.author ?? '',
			narrator: book.narrator ?? '',
			language: book.language ?? '',
			description: book.description ?? '',
			category: book.category ?? '',
			tags: book.tags.join(', ')
		};
		visibility = book.visibility;
		members = [...book.members];
		errorMsg = '';
		editing = true;
	}
	async function save(e: SubmitEvent) {
		e.preventDefault();
		errorMsg = '';
		const res = await fetch(`/api/audiobooks/${book.id}`, {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({
				title: form.title,
				author: form.author || null,
				narrator: form.narrator || null,
				language: form.language || null,
				description: form.description || null,
				category: form.category || null,
				tags: form.tags.split(','),
				visibility,
				// The list is kept while the audiobook is in another mode.
				...(visibility === 'restricted' ? { members: members.map((m) => m.id) } : {})
			})
		});
		if (res.ok) {
			editing = false;
			await invalidateAll();
		} else {
			errorMsg = t('edit.saveFailed');
		}
	}

	let coverInput = $state<HTMLInputElement>();
	async function uploadCover(files: FileList | null) {
		if (!files || files.length === 0) return;
		const body = new FormData();
		body.append('cover', files[0], files[0].name);
		const res = await fetch(`/api/audiobooks/${book.id}/cover`, { method: 'POST', body });
		if (res.ok) await invalidateAll();
		if (coverInput) coverInput.value = '';
	}

	let confirmDelete = $state(false);
	async function remove() {
		confirmDelete = false;
		const res = await fetch(`/api/audiobooks/${book.id}`, { method: 'DELETE' });
		if (res.ok) {
			await invalidateAll();
			goto('/audiobooks');
		}
	}

	// ---- Listening here ----
	let player = $state<HTMLAudioElement>();
	let playing = $state<number | null>(null);
	let paused = $state(true);
	function play(part: AudiobookPart) {
		if (!player) return;
		if (playing === part.id) {
			if (player.paused) player.play();
			else player.pause();
			return;
		}
		playing = part.id;
		player.src = `/api/audiobooks/${book.id}/files/${part.id}/audio`;
		player.play();
	}
	/** On to the next part when one ends. */
	function ended() {
		const next = book.files[book.files.findIndex((p) => p.id === playing) + 1];
		if (next) play(next);
		else playing = null;
	}
	const current = $derived<AudiobookDetail['files'][number] | undefined>(book.files.find((p) => p.id === playing));
</script>

<a class="back" href="/audiobooks"><ArrowLeft size={13} /> {t('audio.heading')}</a>

<div class="top">
	<div class="cover">
		{#if book.has_cover}
			<img src={`/api/audiobooks/${book.id}/cover?v=${encodeURIComponent(book.updated_at ?? '')}`} alt="" />
		{:else}
			<Headphones size={56} strokeWidth={1.2} />
		{/if}
	</div>
	<div class="about">
		{#if editing}
			<form onsubmit={save}>
				<label>{t('shelfEdit.name')}<input bind:value={form.title} required /></label>
				<label>{t('audio.author')}<input bind:value={form.author} /></label>
				<label>{t('audio.narrator')}<input bind:value={form.narrator} /></label>
				<label>{t('book.language')}<input bind:value={form.language} placeholder="sv" /></label>
				<label>{t('shelfEdit.description')}<textarea bind:value={form.description} rows="4"></textarea></label>
				<label>
					{t('edit.category')}
					<input bind:value={form.category} placeholder={t('edit.categoryPlaceholder')} />
				</label>
				<label>
					{t('edit.tags')}
					<input bind:value={form.tags} placeholder={t('edit.tagsPlaceholder')} />
					<span class="hint">{t('edit.tagsHint')}</span>
				</label>
				<fieldset>
					<legend>{t('audio.visibility')}</legend>
					<label class="choice">
						<input type="radio" bind:group={visibility} value="private" />
						<span>
							{t('shelfEdit.visibility.private')}
							<span class="hint">{t('shelfEdit.visibility.privateHint')}</span>
						</span>
					</label>
					<label class="choice">
						<input type="radio" bind:group={visibility} value="restricted" />
						<span>
							{t('shelfEdit.visibility.restricted')}
							<span class="hint">{t('audio.visibility.restrictedHint')}</span>
						</span>
					</label>
					{#if visibility === 'restricted'}
						<MemberPicker bind:members />
					{/if}
					<label class="choice">
						<input type="radio" bind:group={visibility} value="instance" />
						<span>
							{t('shelfEdit.visibility.instance')}
							<span class="hint">{t('audio.visibility.instanceHint')}</span>
						</span>
					</label>
				</fieldset>
				{#if errorMsg}<p class="error">{errorMsg}</p>{/if}
				<div class="row">
					<button type="submit">{t('edit.save')}</button>
					<button type="button" class="ghost" onclick={() => (editing = false)}>{t('common.cancel')}</button>
				</div>
			</form>
		{:else}
			<h1>{book.title || t('audio.untitled')}</h1>
			{#if book.author}<p class="by">{book.author}</p>{/if}
			{#if book.narrator}<p class="narrator">{t('audio.readBy', { name: book.narrator })}</p>{/if}
			<p class="facts">
				{formatLength(book.seconds)} ·
				{book.parts === 1 ? t('audio.part1') : t('audio.partsN', { count: book.parts })} ·
				{formatBytes(book.bytes)}
			</p>
			{#if !book.mine}
				<p class="shared"><Users size={13} /> {t('audio.sharedBy', { owner: book.owner })}</p>
			{:else if book.visibility === 'instance'}
				<p class="shared"><Users size={13} /> {t('audio.sharedInstance')}</p>
			{:else if book.visibility === 'restricted'}
				<p class="shared">
					<Users size={13} />
					{t('audio.sharedWith', { names: book.members.map((m) => m.username).join(', ') || '–' })}
				</p>
			{/if}
			{#if book.category || book.tags.length > 0}
				<div class="chips">
					{#if book.category}
						<span class="chip category" title={t('book.category')}>
							<Layers size={11} />
							{book.category}
						</span>
					{/if}
					{#each book.tags as tag (tag)}
						<span class="chip" title={t('book.tags')}>
							<Tag size={11} />
							{tag}
						</span>
					{/each}
				</div>
			{/if}
			{#if book.description}<p class="description">{book.description}</p>{/if}
			{#if book.mine}
			<div class="row">
				<button type="button" class="ghost" onclick={startEdit}><Pencil size={13} /> {t('book.edit')}</button>
				<button type="button" class="ghost" onclick={() => coverInput?.click()}>
					<ImagePlus size={13} />
					{t('shelfEdit.chooseCover')}
				</button>
				<input type="file" accept="image/*" hidden bind:this={coverInput} onchange={(e) => uploadCover(e.currentTarget.files)} />
			</div>
			{/if}
		{/if}
	</div>
</div>

<section class="feed">
	<h2><Podcast size={16} /> {t('audio.feedHeading')}</h2>
	<p>{t('audio.feedHint')}</p>
	<div class="feed-row">
		<input readonly value={book.feed_url} onfocus={(e) => e.currentTarget.select()} aria-label={t('audio.feedHeading')} />
		<button type="button" class="ghost" onclick={copyFeed}>
			{#if copied}<Check size={13} /> {t('kobo.copied')}{:else}<Copy size={13} /> {t('audio.copy')}{/if}
		</button>
	</div>
	<div class="apps">
		<span>{t('audio.openIn')}</span>
		{#each apps as app (app.name)}
			<a class="button" href={app.href}>{app.name}</a>
		{/each}
	</div>
	<p class="small">
		{t('audio.feedSecret')}
		<button type="button" class="link" onclick={() => (confirmKey = true)}>
			<RefreshCw size={11} />
			{t('audio.newKey')}
		</button>
	</p>
</section>

<section class="parts">
	<h2>{t('audio.parts')}</h2>
	<!-- svelte-ignore a11y_media_has_caption -->
	<audio
		bind:this={player}
		controls={playing !== null}
		class:idle={playing === null}
		onended={ended}
		onplay={() => (paused = false)}
		onpause={() => (paused = true)}
		aria-label={current ? partTitle(current) : undefined}
	></audio>
	{#if book.files.length === 0}
		<p class="small">{t('audio.noParts')}</p>
	{:else}
		<ol>
			{#each book.files as part (part.id)}
				<li class:on={playing === part.id}>
					<button
						type="button"
						class="play"
						onclick={() => play(part)}
						aria-label={playing === part.id && !paused ? t('reader.speech.pause') : t('audio.play', { title: partTitle(part) })}
					>
						{#if playing === part.id && !paused}<Pause size={14} />{:else}<Play size={14} />{/if}
					</button>
					<span class="n">{part.position}</span>
					<span class="title">{partTitle(part)}</span>
					<span class="time">{part.seconds > 0 ? clock(part.seconds) : formatBytes(part.bytes)}</span>
				</li>
			{/each}
		</ol>
	{/if}
</section>

{#if book.mine}
	<div class="danger-zone">
		<button type="button" class="ghost danger" onclick={() => (confirmDelete = true)}>
			<Trash2 size={13} />
			{t('audio.delete')}
		</button>
	</div>
{/if}

<ConfirmDialog
	open={confirmDelete}
	title={t('audio.delete')}
	message={t('audio.deleteConfirm', { name: book.title })}
	confirmLabel={t('audio.delete')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={remove}
	oncancel={() => (confirmDelete = false)}
/>
<ConfirmDialog
	open={confirmKey}
	title={t('audio.newKey')}
	message={t('audio.newKeyConfirm')}
	confirmLabel={t('audio.newKey')}
	cancelLabel={t('common.cancel')}
	onconfirm={newKey}
	oncancel={() => (confirmKey = false)}
/>

<style>
	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		margin-bottom: 1rem;
		color: var(--muted);
	}
	.top {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-start;
		gap: 1.5rem;
	}
	.cover {
		width: 12rem;
		aspect-ratio: 1;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
		background: var(--card);
		color: var(--gold);
		box-shadow: var(--shadow);
	}
	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.about {
		flex: 1;
		min-width: 16rem;
	}
	h1 {
		font-size: 1.5rem;
		margin: 0 0 0.3rem;
	}
	h2 {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 1.05rem;
		margin: 0 0 0.5rem;
	}
	.by {
		margin: 0;
	}
	.narrator,
	.facts {
		margin: 0.15rem 0 0;
		color: var(--muted);
		font-size: 0.9rem;
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		margin-top: 0.7rem;
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.8rem;
		padding: 0.15rem 0.6rem;
		border-radius: 99px;
		border: 1px solid var(--border);
		color: var(--muted);
	}
	.chip.category {
		border-color: var(--accent);
		color: var(--accent);
	}
	.description {
		margin: 0.8rem 0 0;
		white-space: pre-line;
		max-width: 40rem;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin-top: 1rem;
	}
	.row button,
	.feed-row button,
	.danger {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.82rem;
	}
	form {
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
		max-width: 30rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.88rem;
		color: var(--muted);
	}
	.hint {
		font-size: 0.78rem;
	}
	fieldset {
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 0.75rem 1rem 1rem;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
	}
	legend {
		font-size: 0.88rem;
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
	input[type='radio'] {
		width: auto;
		margin: 0.2rem 0 0;
		accent-color: var(--accent);
	}
	label.choice > span {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}
	label.choice .hint {
		color: var(--muted);
	}
	.shared {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		margin: 0.5rem 0 0;
		font-size: 0.85rem;
		color: var(--gold);
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
	section {
		margin-top: 2rem;
		max-width: 44rem;
	}
	.feed {
		padding: 0.9rem 1rem 1rem;
		border: 1px solid var(--border);
		border-left: 3px solid var(--gold, var(--accent));
		border-radius: 8px;
		background: var(--card);
	}
	.feed p {
		margin: 0 0 0.6rem;
		font-size: 0.9rem;
	}
	.feed-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.feed-row input {
		flex: 1;
		min-width: 14rem;
		font-family: ui-monospace, 'SF Mono', monospace;
		font-size: 0.8rem;
	}
	.apps {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		margin-top: 0.6rem;
		font-size: 0.85rem;
		color: var(--muted);
	}
	.button {
		display: inline-flex;
		align-items: center;
		padding: 0.35rem 0.8rem;
		border-radius: 6px;
		background: var(--accent);
		color: var(--bg);
		font-size: 0.82rem;
	}
	.button:hover {
		text-decoration: none;
	}
	.small {
		margin: 0.6rem 0 0 !important;
		font-size: 0.8rem !important;
		color: var(--muted);
	}
	.link {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		background: none;
		border: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		cursor: pointer;
	}
	audio {
		width: 100%;
		margin-bottom: 0.6rem;
	}
	audio.idle {
		display: none;
	}
	ol {
		list-style: none;
		margin: 0;
		padding: 0;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--card);
	}
	li {
		display: grid;
		grid-template-columns: 2rem 1.8rem 1fr auto;
		align-items: center;
		gap: 0.4rem;
		padding: 0.35rem 0.75rem 0.35rem 0.4rem;
		border-top: 1px solid var(--border);
		font-size: 0.9rem;
	}
	li:first-child {
		border-top: none;
	}
	li.on .title {
		font-weight: 600;
		color: var(--accent);
	}
	.play {
		padding: 0.35rem;
		border: none;
		background: none;
		color: var(--accent);
		display: inline-flex;
	}
	.n,
	.time {
		color: var(--muted);
		font-size: 0.8rem;
		font-variant-numeric: tabular-nums;
	}
	.n {
		text-align: right;
	}
	.title {
		overflow-wrap: anywhere;
	}
	.danger-zone {
		margin-top: 2.5rem;
		padding-top: 1.25rem;
		border-top: 1px solid var(--border);
		max-width: 44rem;
	}
	.danger {
		color: var(--danger);
		border-color: var(--danger);
	}
</style>
