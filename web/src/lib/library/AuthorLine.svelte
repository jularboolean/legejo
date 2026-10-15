<script lang="ts">
	// One quiet line about an author above their books, from Wikidata by way
	// of the server: who they were, when they lived, and where Wikipedia has
	// more. Shown only once the server knows the person for certain; until
	// then there is nothing, so the books never move under the reader.
	import { getLocale, t } from '#lib/i18n';

	let { name }: { name: string } = $props();

	type Line = {
		state: string;
		name?: string;
		born?: number | null;
		died?: number | null;
		description?: string | null;
		wikipedia?: { lang: string; url: string } | null;
	};

	let line = $state<Line | null>(null);

	$effect(() => {
		const asked = name;
		line = null;
		const query = new URLSearchParams({ name: asked, lang: getLocale() });
		fetch(`/api/authors/facts?${query}`)
			.then((res) => (res.ok ? res.json() : null))
			.then((got: Line | null) => {
				if (asked === name && got?.state === 'ok') line = got;
			})
			.catch(() => {
				// Without the server there is no line, and nothing is missed.
			});
	});

	const year = (y: number) => (y < 0 ? t('authors.bce', { year: -y }) : String(y));
	const years = $derived.by(() => {
		if (!line?.born && !line?.died) return null;
		if (line.born && line.died) return `${year(line.born)}–${year(line.died)}`;
		return line.born ? t('authors.born', { year: year(line.born) }) : t('authors.died', { year: year(line.died!) });
	});

	async function wrongPerson() {
		line = null;
		await fetch('/api/authors/facts/hidden', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ name })
		}).catch(() => {});
	}
</script>

{#if line}
	<p class="author-line">
		<span class="who">{line.name}</span>
		{#if years}<span class="sep">·</span><span>{years}</span>{/if}
		{#if line.description}<span class="sep">·</span><span>{line.description}</span>{/if}
		{#if line.wikipedia}
			<span class="sep">·</span>
			<a href={line.wikipedia.url} target="_blank" rel="noopener noreferrer" lang={line.wikipedia.lang}>{t('authors.wikipedia')} ↗</a>
		{/if}
		<button type="button" class="wrong" onclick={wrongPerson}>{t('authors.wrongPerson')}</button>
	</p>
{/if}

<style>
	.author-line {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.15rem 0.45rem;
		margin: -0.25rem 0 1.1rem;
		font-size: 0.86rem;
		color: var(--muted);
	}
	.who {
		color: var(--fg);
		font-weight: 600;
	}
	.sep {
		opacity: 0.6;
	}
	/* The way out is there for whoever looks for it, and quiet otherwise. */
	.wrong {
		margin-left: 0.4rem;
		padding: 0;
		background: none;
		border: none;
		color: var(--muted);
		font: inherit;
		font-size: 0.8rem;
		font-weight: 400;
		letter-spacing: 0;
		text-decoration: underline;
		opacity: 0;
		transition: opacity 0.15s;
	}
	.author-line:hover .wrong,
	.wrong:focus-visible {
		opacity: 1;
	}
	@media (hover: none) {
		.wrong {
			opacity: 0.7;
		}
	}
</style>
