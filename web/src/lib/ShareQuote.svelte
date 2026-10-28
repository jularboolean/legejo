<script lang="ts">
	// A highlight to post elsewhere, as a picture: the card is drawn here, in
	// the browser, and handed to the device's share sheet, the clipboard or a
	// download, with words to go beside it and a description of the picture.
	import { Check, ClipboardCopy, Download, Share2, X } from '@lucide/svelte';
	import { fade, pop } from '#lib/motion';
	import { t } from '#lib/i18n';
	import { altText, drawCard, fileName, loadFaces, postText } from '#lib/quotecard';

	type Props = {
		open: boolean;
		quote: string;
		/** The highlight's note: the words for the post, to begin with. */
		note: string | null;
		title: string;
		author: string | null;
		/** The book's cover, or null for a book without one. */
		coverUrl: string | null;
		color?: string | null;
		onclose: () => void;
	};
	let { open, quote, note, title, author, coverUrl, color = null, onclose }: Props = $props();

	let canvas = $state<HTMLCanvasElement | null>(null);
	let file = $state<File | null>(null);
	let picture = $state<string | null>(null);
	let failed = $state(false);
	let comment = $state('');
	let done = $state<'image' | 'text' | 'alt' | null>(null);
	let closeBtn = $state<HTMLButtonElement | null>(null);

	const from = $derived(author ? t('share.from', { title, author }) : t('share.fromTitle', { title }));
	const words = $derived(postText(comment, from));
	const description = $derived(altText(quote, title, author));
	const canShare = $derived(file !== null && typeof navigator !== 'undefined' && !!navigator.canShare?.({ files: [file] }));
	const canCopyImage = typeof window !== 'undefined' && 'ClipboardItem' in window && !!navigator.clipboard?.write;

	async function loadCover(url: string): Promise<HTMLImageElement | null> {
		const img = new Image();
		img.src = url;
		try {
			await img.decode();
			return img;
		} catch {
			return null;
		}
	}

	async function draw(target: HTMLCanvasElement) {
		failed = false;
		const [cover] = await Promise.all([coverUrl ? loadCover(coverUrl) : null, loadFaces(quote + title + (author ?? ''))]);
		drawCard(target, {
			quote,
			title,
			author,
			cover,
			coverRatio: cover ? cover.naturalWidth / cover.naturalHeight : undefined,
			color
		});
		const blob = await new Promise<Blob | null>((resolve) => target.toBlob(resolve, 'image/png'));
		if (!blob) {
			failed = true;
			return;
		}
		file = new File([blob], fileName(title), { type: 'image/png' });
		picture = URL.createObjectURL(blob);
	}

	// Drawn anew each time it is opened, for the highlight it is opened on.
	$effect(() => {
		if (!open || !canvas) return;
		comment = note ?? '';
		done = null;
		closeBtn?.focus();
		void draw(canvas);
		return () => {
			if (picture) URL.revokeObjectURL(picture);
			picture = null;
			file = null;
		};
	});

	function flash(what: typeof done) {
		done = what;
		setTimeout(() => {
			if (done === what) done = null;
		}, 1800);
	}

	async function share() {
		if (!file) return;
		try {
			await navigator.share({ files: [file], text: words });
		} catch {
			// Cancelled in the share sheet, or refused: nothing to say.
		}
	}

	async function copyImage() {
		if (!file) return;
		try {
			await navigator.clipboard.write([new ClipboardItem({ 'image/png': file })]);
			flash('image');
		} catch {
			failed = true;
		}
	}

	async function copy(text: string, what: 'text' | 'alt') {
		try {
			await navigator.clipboard.writeText(text);
			flash(what);
		} catch {
			// No clipboard here: the text can still be selected by hand.
		}
	}

	function onkeydown(e: KeyboardEvent) {
		if (open && e.key === 'Escape') {
			e.preventDefault();
			onclose();
		}
	}
</script>

<svelte:window {onkeydown} />

{#if open}
	<div class="backdrop" onclick={onclose} role="presentation" transition:fade={{ duration: 160 }}>
		<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
		<div class="dialog" role="dialog" aria-modal="true" aria-label={t('share.heading')} tabindex="-1" onclick={(e) => e.stopPropagation()} in:pop>
			<div class="head">
				<h2>{t('share.heading')}</h2>
				<button type="button" class="ghost icon" bind:this={closeBtn} onclick={onclose} aria-label={t('common.close')} title={t('common.close')}>
					<X size={18} />
				</button>
			</div>

			<canvas bind:this={canvas} hidden></canvas>
			<div class="picture">
				{#if picture}
					<img src={picture} alt={description} />
				{:else if failed}
					<p class="error">{t('share.failed')}</p>
				{:else}
					<p class="waiting">{t('share.drawing')}</p>
				{/if}
			</div>

			<div class="buttons">
				{#if canShare}
					<button type="button" onclick={share}><Share2 size={16} /> {t('share.share')}</button>
				{/if}
				{#if canCopyImage}
					<button type="button" class:ghost={canShare} disabled={!file} onclick={copyImage}>
						{#if done === 'image'}<Check size={16} /> {t('share.copied')}{:else}<ClipboardCopy size={16} /> {t('share.copyImage')}{/if}
					</button>
				{/if}
				<a class="button ghost" class:disabled={!picture} href={picture ?? undefined} download={file?.name}><Download size={16} /> {t('share.download')}</a>
			</div>

			<label class="field">
				<span>{t('share.words')}</span>
				<textarea bind:value={comment} rows="2" placeholder={t('share.wordsPlaceholder')}></textarea>
			</label>
			<div class="copyline">
				<p class="preview">{words}</p>
				<button type="button" class="ghost small" onclick={() => copy(words, 'text')}>
					{done === 'text' ? t('share.copied') : t('share.copyText')}
				</button>
			</div>
			<div class="copyline">
				<p class="preview muted">{t('share.altHint')}</p>
				<button type="button" class="ghost small" onclick={() => copy(description, 'alt')}>
					{done === 'alt' ? t('share.copied') : t('share.copyAlt')}
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		background: rgba(20, 16, 10, 0.45);
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 1rem;
		z-index: 100;
	}
	.dialog {
		background: var(--card);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 10px;
		box-shadow: var(--shadow);
		padding: 1rem 1.25rem 1.25rem;
		width: 100%;
		max-width: 32rem;
		max-height: calc(100dvh - 2rem);
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 0.8rem;
		font-family: 'Inter Variable', system-ui, sans-serif;
		font-size: 0.92rem;
	}
	/* The dialog scrolls; its parts keep their height. */
	.dialog > * {
		flex-shrink: 0;
	}
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	h2 {
		margin: 0;
		font-size: 1.15rem;
	}
	.icon {
		padding: 0.3rem;
		display: inline-flex;
	}
	.picture {
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 10rem;
		border-radius: 6px;
		background: var(--bg);
	}
	.picture img {
		display: block;
		max-width: 100%;
		max-height: 50dvh;
		border-radius: 6px;
		box-shadow: 0 1px 6px rgba(0, 0, 0, 0.15);
	}
	.waiting,
	.muted {
		color: var(--muted);
	}
	.error {
		color: var(--danger);
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.buttons button,
	.buttons .button {
		font-size: 0.85rem;
		padding: 0.45rem 0.8rem;
	}
	/* The download link, dressed as the ghost button beside it. */
	.button {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		font-weight: 600;
		letter-spacing: 0.03em;
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--muted);
		text-decoration: none;
	}
	.button:hover {
		color: var(--fg);
		border-color: var(--muted);
	}
	.button.disabled {
		pointer-events: none;
		opacity: 0.5;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.field span {
		font-weight: 500;
	}
	textarea {
		width: 100%;
		font: inherit;
		padding: 0.5rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg);
		color: var(--fg);
		resize: vertical;
	}
	.copyline {
		display: flex;
		align-items: flex-start;
		gap: 0.75rem;
	}
	.preview {
		flex: 1;
		min-width: 0;
		margin: 0;
		white-space: pre-wrap;
		font-size: 0.85rem;
	}
	.small {
		flex-shrink: 0;
		padding: 0.25rem 0.6rem;
		font-size: 0.82rem;
	}
</style>
