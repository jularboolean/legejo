<script lang="ts">
	import { Check } from '@lucide/svelte';
	import { DESIGNS, design, setDesign, type Design } from '#lib/design.svelte';
	import { t } from '#lib/i18n';
	import type { MessageKey } from '#lib/i18n';

	const name = (d: Design) => t(`design.${d}` as MessageKey);
	const about = (d: Design) => t(`design.${d}.about` as MessageKey);
</script>

<section>
	<h2>{t('design.heading')}</h2>
	<p class="intro">{t('design.intro')}</p>
	<div class="choices" role="radiogroup" aria-label={t('design.heading')}>
		{#each DESIGNS as d (d)}
			<button
				type="button"
				role="radio"
				aria-checked={design() === d}
				class="choice"
				onclick={() => setDesign(d)}
			>
				<span class="sample {d}" aria-hidden="true">
					<span class="page">
						<span class="line title"></span>
						<span class="line"></span>
						<span class="line short"></span>
						<span class="dot"></span>
					</span>
				</span>
				<span class="text">
					<span class="name">
						{name(d)}
						{#if design() === d}<Check size={14} strokeWidth={3} />{/if}
					</span>
					<span class="about">{about(d)}</span>
				</span>
			</button>
		{/each}
	</div>
</section>

<style>
	h2 {
		font-size: 1.1rem;
		margin: 0 0 0.5rem;
	}
	.intro {
		color: var(--muted);
		font-size: 0.9rem;
		margin: 0 0 1rem;
		max-width: 36rem;
	}
	.choices {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr));
		gap: 0.9rem;
		max-width: 52rem;
	}
	.choice {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 0.6rem;
		padding: 0.6rem;
		background: var(--card);
		color: var(--fg);
		border: 1.5px solid var(--border);
		border-radius: 12px;
		text-align: left;
		font-weight: 400;
		letter-spacing: 0;
	}
	.choice:hover {
		filter: none;
		border-color: var(--muted);
	}
	.choice[aria-checked='true'] {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent);
	}
	.text {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		padding: 0 0.2rem 0.2rem;
	}
	.name {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-weight: 600;
	}
	.choice[aria-checked='true'] .name {
		color: var(--accent);
	}
	.about {
		font-size: 0.8rem;
		color: var(--muted);
		line-height: 1.35;
	}

	/* A small page in each design's own colours, whatever design is on. */
	.sample {
		--s-bg: #f3f5f5;
		--s-card: #fdfefe;
		--s-fg: #1d2224;
		--s-muted: #b9c3c5;
		--s-accent: #53676c;
		display: block;
		aspect-ratio: 16 / 10;
		border-radius: 8px;
		padding: 0.7rem 0.7rem 0;
		background: var(--s-bg);
		overflow: hidden;
		border: 1px solid rgb(0 0 0 / 0.08);
	}
	.sample.dark {
		--s-bg: #15181a;
		--s-card: #1d2124;
		--s-fg: #e4e8e9;
		--s-muted: #48545a;
		--s-accent: #94b0b8;
	}
	.sample.zamenhof {
		--s-bg: #ece0c8;
		--s-card: #f7efdd;
		--s-fg: #3b2a1a;
		--s-muted: #c9b48f;
		--s-accent: #2f6b3c;
	}
	.sample.auto {
		background: linear-gradient(115deg, #f3f5f5 0 50%, #15181a 50% 100%);
	}
	.page {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		height: 100%;
		padding: 0.6rem;
		border-radius: 6px 6px 0 0;
		background: var(--s-card);
		box-shadow: 0 1px 4px rgb(0 0 0 / 0.15);
	}
	.sample.auto .page {
		background: linear-gradient(115deg, #fdfefe 0 50%, #1d2124 50% 100%);
	}
	.line {
		height: 0.32rem;
		border-radius: 99px;
		background: var(--s-muted);
		width: 80%;
	}
	.line.title {
		height: 0.5rem;
		width: 55%;
		background: var(--s-fg);
		margin-bottom: 0.15rem;
	}
	.sample.auto .line.title {
		background: #8d9a9e;
	}
	.line.short {
		width: 45%;
	}
	.dot {
		position: absolute;
		right: 0.6rem;
		top: 0.6rem;
		width: 0.9rem;
		height: 0.9rem;
		border-radius: 50%;
		background: var(--s-accent);
	}
	.sample.zamenhof .dot {
		border-radius: 0;
		clip-path: polygon(50% 0, 61% 35%, 98% 35%, 68% 57%, 79% 91%, 50% 70%, 21% 91%, 32% 57%, 2% 35%, 39% 35%);
	}
</style>
