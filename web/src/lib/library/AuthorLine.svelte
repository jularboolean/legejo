<script lang="ts">
	// Who an author was, beside the chip that filters the library on them:
	// years, a description and a link to Wikipedia, from Wikidata by way of
	// the server. The chip already says the name. It sits in a row that is
	// there anyway, so nothing below moves when it arrives; the first time
	// an author is opened the server is still asking, and is asked again a
	// few times while it does.
	import { getLocale, t } from '#lib/i18n';

	let { name }: { name: string } = $props();

	type Line = {
		state: string;
		born?: number | null;
		died?: number | null;
		description?: string | null;
		wikipedia?: { lang: string; url: string } | null;
	};

	/** How long to wait before asking again while the server looks it up. */
	const AGAIN_MS = [1200, 2500, 5000];

	let line = $state<Line | null>(null);
	/** The line just called the wrong person, for a few seconds' undo. */
	let undoable = $state<Line | null>(null);
	let undoTimer: ReturnType<typeof setTimeout> | null = null;
	/** The user has said, some other time, that this is the wrong person. */
	let hiddenBefore = $state(false);
	/** Bumped to ask again from the start, as after "show again". */
	let round = $state(0);
	const UNDO_MS = 6000;

	// The undo's timer goes with the line, whenever the line goes.
	$effect(() => () => {
		if (undoTimer) clearTimeout(undoTimer);
	});

	$effect(() => {
		const asked = name;
		void round;
		line = null;
		undoable = null;
		hiddenBefore = false;
		let timer: ReturnType<typeof setTimeout> | null = null;
		let gone = false;
		const ask = (attempt: number) => {
			const query = new URLSearchParams({ name: asked, lang: getLocale() });
			fetch(`/api/authors/facts?${query}`)
				.then((res) => (res.ok ? res.json() : null))
				.then((got: Line | null) => {
					if (gone) return;
					if (got?.state === 'ok') line = got;
					else if (got?.state === 'hidden') hiddenBefore = true;
					else if (got?.state === 'pending' && attempt < AGAIN_MS.length) {
						timer = setTimeout(() => ask(attempt + 1), AGAIN_MS[attempt]);
					}
				})
				.catch(() => {
					// Without the server there is no line, and nothing is missed.
				});
		};
		ask(0);
		return () => {
			gone = true;
			if (timer) clearTimeout(timer);
		};
	});

	const year = (y: number) => (y < 0 ? t('authors.bce', { year: -y }) : String(y));
	const years = $derived.by(() => {
		if (!line?.born && !line?.died) return null;
		if (line.born && line.died) return `${year(line.born)}–${year(line.died)}`;
		return line.born ? t('authors.born', { year: year(line.born) }) : t('authors.died', { year: year(line.died!) });
	});

	function hidden(method: 'POST' | 'DELETE') {
		return fetch('/api/authors/facts/hidden', {
			method,
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ name })
		}).catch(() => {});
	}

	function wrongPerson() {
		undoable = line;
		line = null;
		hidden('POST');
		if (undoTimer) clearTimeout(undoTimer);
		undoTimer = setTimeout(() => (undoable = null), UNDO_MS);
	}

	function undo() {
		if (undoTimer) clearTimeout(undoTimer);
		line = undoable;
		undoable = null;
		hidden('DELETE');
	}

	/** Hidden some other time: shown again, as it is now. */
	async function showAgain() {
		hiddenBefore = false;
		await hidden('DELETE');
		round++;
	}
</script>

{#if line && (years || line.description || line.wikipedia)}
	<span class="author-note">
		{#if years}<span>{years}</span>{/if}
		{#if line.description}
			{#if years}<span class="sep">·</span>{/if}
			<span>{line.description}</span>
		{/if}
		{#if line.wikipedia}
			<span class="sep">·</span>
			<a href={line.wikipedia.url} target="_blank" rel="noopener noreferrer" lang={line.wikipedia.lang}>{t('authors.wikipedia')} ↗</a>
		{/if}
		<button type="button" class="wrong" onclick={wrongPerson}>{t('authors.wrongPerson')}</button>
	</span>
{:else if hiddenBefore}
	<span class="author-note quiet">
		<span>{t('authors.hiddenByYou')}</span>
		<span class="sep">·</span>
		<button type="button" class="undo" onclick={showAgain}>{t('authors.showAgain')}</button>
	</span>
{:else if undoable}
	<span class="author-note" role="status">
		<span>{t('authors.hidden')}</span>
		<span class="sep">·</span>
		<button type="button" class="undo" onclick={undo}>{t('authors.undo')}</button>
	</span>
{/if}

<style>
	.author-note {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.1rem 0.4rem;
		margin: 0 0.6rem 0 0.2rem;
		font-size: 0.84rem;
		color: var(--muted);
	}
	/* With animations on, it comes in slowly, so that it is noticed rather
	   than jumping out. */
	:global(:root[data-motion='on']) .author-note {
		animation: arrive 0.6s ease-out;
	}
	@keyframes arrive {
		from {
			opacity: 0;
		}
	}
	.sep {
		opacity: 0.6;
	}
	/* A line once hidden says so in a whisper, with the way back. */
	.quiet {
		font-size: 0.78rem;
		opacity: 0.75;
	}
	.undo {
		padding: 0;
		background: none;
		border: none;
		color: var(--accent);
		font: inherit;
		font-weight: 600;
		letter-spacing: 0;
		text-decoration: underline;
	}
	/* The way out is there for whoever looks for it, and quiet otherwise. */
	.wrong {
		margin-left: 0.3rem;
		padding: 0;
		background: none;
		border: none;
		color: var(--muted);
		font: inherit;
		font-size: 0.78rem;
		font-weight: 400;
		letter-spacing: 0;
		text-decoration: underline;
		opacity: 0;
		transition: opacity 0.15s;
	}
	.author-note:hover .wrong,
	.wrong:focus-visible {
		opacity: 1;
	}
	@media (hover: none) {
		.wrong {
			opacity: 0.7;
		}
	}
</style>
