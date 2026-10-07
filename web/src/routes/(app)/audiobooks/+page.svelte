<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { FolderUp, Headphones, Upload } from '@lucide/svelte';
	import { audioFiles, formatLength, sendPart } from '#lib/audio';
	import { t } from '#lib/i18n';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	let fileInput = $state<HTMLInputElement>();
	let folderInput = $state<HTMLInputElement>();
	/** The upload under way: how many files, how many bytes, how far. */
	let upload = $state<{ name: string; index: number; count: number; sent: number; total: number } | null>(null);
	let message = $state('');

	/** One selection becomes one audiobook, its files sent one at a time, in order. */
	async function add(selection: FileList | null) {
		const files = audioFiles(selection ?? []);
		message = '';
		if (files.length === 0) {
			if (selection && selection.length > 0) message = t('audio.noAudio');
			return;
		}
		const total = files.reduce((n, f) => n + f.size, 0);
		upload = { name: files[0].name, index: 1, count: files.length, sent: 0, total };
		let id: number | null = null;
		try {
			const res = await fetch('/api/audiobooks', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({})
			});
			if (!res.ok) throw new Error();
			id = (await res.json()).id as number;
			let before = 0;
			for (const [i, file] of files.entries()) {
				upload = { name: file.name, index: i + 1, count: files.length, sent: before, total };
				const result = await sendPart(id, file, (sent) => {
					if (upload) upload.sent = before + sent;
				});
				if (!result.ok) {
					message =
						result.error === 'too-large'
							? t('audio.tooLarge', { file: file.name })
							: t('audio.partFailed', { file: file.name });
					break;
				}
				before += file.size;
			}
			await invalidateAll();
			await goto(`/audiobooks/${id}`);
		} catch {
			message = t('audio.uploadFailed');
		} finally {
			upload = null;
			if (fileInput) fileInput.value = '';
			if (folderInput) folderInput.value = '';
		}
	}

	const percent = $derived(upload && upload.total > 0 ? Math.round((upload.sent / upload.total) * 100) : 0);
</script>

<div class="heading">
	<h1>{t('audio.heading')} <span class="count">{data.audiobooks.length}</span></h1>
	<div class="buttons">
		<button type="button" class="ghost" onclick={() => folderInput?.click()} disabled={upload !== null}>
			<FolderUp size={13} />
			{t('audio.addFolder')}
		</button>
		<button type="button" onclick={() => fileInput?.click()} disabled={upload !== null}>
			<Upload size={13} />
			{t('audio.addFiles')}
		</button>
	</div>
	<input
		type="file"
		accept=".mp3,.m4a,.m4b,audio/mpeg,audio/mp4"
		multiple
		hidden
		bind:this={fileInput}
		onchange={(e) => add(e.currentTarget.files)}
	/>
	<input type="file" webkitdirectory multiple hidden bind:this={folderInput} onchange={(e) => add(e.currentTarget.files)} />
</div>

{#if upload}
	<section class="progress" aria-label={t('upload.heading')}>
		<p role="status">
			{t('audio.uploading', { index: upload.index, count: upload.count })}
			<span class="file">{upload.name}</span>
		</p>
		<div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent}>
			<div class="fill" style:width={`${percent}%`}></div>
		</div>
	</section>
{/if}

{#if message}<p class="error">{message}</p>{/if}

{#if data.audiobooks.length === 0 && !upload}
	<p class="empty">{t('audio.empty')}</p>
	<p class="hint">{t('audio.emptyHint')}</p>
{:else}
	<div class="grid">
		{#each data.audiobooks as book (book.id)}
			<a class="card" href={`/audiobooks/${book.id}`}>
				<div class="cover">
					{#if book.has_cover}
						<img src={`/api/audiobooks/${book.id}/cover?v=${encodeURIComponent(book.updated_at ?? '')}`} alt="" loading="lazy" />
					{:else}
						<Headphones size={34} strokeWidth={1.4} />
					{/if}
				</div>
				<div class="meta">
					<span class="name">{book.title || t('audio.untitled')}</span>
					{#if book.author}<span class="by">{book.author}</span>{/if}
					<span class="length">
						{formatLength(book.seconds)} · {book.parts === 1 ? t('audio.part1') : t('audio.partsN', { count: book.parts })}
					</span>
				</div>
			</a>
		{/each}
	</div>
{/if}

<style>
	.heading {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 0.75rem;
		margin-bottom: 1.25rem;
	}
	h1 {
		font-size: 1.4rem;
		margin: 0;
	}
	.count {
		color: var(--muted);
		font-family: 'Inter Variable', system-ui, sans-serif;
		font-weight: 400;
		font-size: 0.95rem;
		margin-left: 0.3rem;
	}
	.buttons {
		display: flex;
		gap: 0.5rem;
	}
	.buttons button {
		font-size: 0.78rem;
		padding: 0.32rem 0.7rem;
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
	}
	.progress {
		background: var(--card);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 0.7rem 0.9rem;
		margin-bottom: 1rem;
		font-size: 0.88rem;
	}
	.progress p {
		margin: 0 0 0.5rem;
		font-weight: 500;
	}
	.file {
		font-weight: 400;
		color: var(--muted);
		margin-left: 0.4rem;
		overflow-wrap: anywhere;
	}
	.bar {
		height: 4px;
		border-radius: 2px;
		background: var(--border);
		overflow: hidden;
	}
	.fill {
		height: 100%;
		background: var(--accent);
		transition: width 0.2s linear;
	}
	.empty {
		color: var(--muted);
		margin: 0;
	}
	.hint {
		color: var(--muted);
		font-size: 0.88rem;
		max-width: 38rem;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr));
		gap: 1.25rem;
	}
	.card {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
		background: var(--card);
		color: var(--fg);
		box-shadow: var(--shadow);
	}
	.card:hover {
		text-decoration: none;
		border-color: var(--accent);
	}
	.cover {
		aspect-ratio: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg);
		color: var(--gold);
		border-bottom: 1px solid var(--border);
	}
	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.meta {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		padding: 0.6rem 0.8rem 0.7rem;
	}
	.name {
		font-weight: 600;
		font-size: 0.95rem;
	}
	.by,
	.length {
		font-size: 0.8rem;
		color: var(--muted);
	}
	@media (prefers-reduced-motion: reduce) {
		.fill {
			transition: none;
		}
	}
</style>
