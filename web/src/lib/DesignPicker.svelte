<script lang="ts">
	import { Check } from '@lucide/svelte';
	import { DESIGNS, design, motion, setDesign, setMotion, type Design } from '#lib/design.svelte';
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
				<span class="swatch {d}" aria-hidden="true"><i></i><i></i><i></i></span>
				<span class="text">
					<span class="name">{name(d)}</span>
					<span class="about">{about(d)}</span>
				</span>
				<span class="mark">{#if design() === d}<Check size={15} strokeWidth={3} />{/if}</span>
			</button>
		{/each}
	</div>
</section>

<section>
	<h2>{t('design.motion')}</h2>
	<label class="switch">
		<input type="checkbox" checked={motion()} onchange={(e) => setMotion(e.currentTarget.checked)} />
		<span class="text">
			<span class="name">{t('design.motion.on')}</span>
			<span class="about">{t('design.motion.about')}</span>
		</span>
	</label>
</section>

<style>
	section + section {
		margin-top: 2rem;
	}
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
		max-width: 30rem;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--card);
		overflow: hidden;
	}
	.choice {
		display: flex;
		align-items: center;
		gap: 0.9rem;
		width: 100%;
		padding: 0.7rem 0.9rem;
		background: none;
		color: var(--fg);
		border: none;
		border-radius: 0;
		text-align: left;
		font-weight: 400;
		letter-spacing: 0;
	}
	.choice + .choice {
		border-top: 1px solid var(--border);
	}
	.choice:hover {
		filter: none;
		background: color-mix(in srgb, var(--bg) 60%, transparent);
	}
	.text {
		display: flex;
		flex-direction: column;
		gap: 0.05rem;
		min-width: 0;
		flex: 1;
	}
	.name {
		font-weight: 600;
		font-size: 0.92rem;
	}
	.about {
		font-size: 0.8rem;
		color: var(--muted);
		line-height: 1.35;
	}
	.mark {
		display: inline-flex;
		width: 15px;
		color: var(--accent);
	}
	.choice[aria-checked='true'] .name {
		color: var(--accent);
	}

	/* Three overlapping discs in each design's own colours: page, card, accent. */
	.swatch {
		--a: #f3f5f5;
		--b: #fdfefe;
		--c: #53676c;
		display: inline-flex;
		flex-shrink: 0;
	}
	.swatch i {
		width: 1.35rem;
		height: 1.35rem;
		border-radius: 50%;
		border: 1px solid color-mix(in srgb, var(--fg) 22%, transparent);
		background: var(--a);
	}
	.swatch i + i {
		margin-left: -0.5rem;
		background: var(--b);
	}
	.swatch i + i + i {
		background: var(--c);
	}
	.swatch.dark {
		--a: #15181a;
		--b: #333a3d;
		--c: #94b0b8;
	}
	.swatch.zamenhof {
		--a: #e8dabd;
		--b: #806a4f;
		--c: #1e8a3e;
	}
	.swatch.light {
		--b: #dbe1e2;
	}
	.swatch.auto i:first-child {
		background: linear-gradient(90deg, #f3f5f5 50%, #15181a 50%);
	}
	.swatch.auto i + i {
		background: linear-gradient(90deg, #dbe1e2 50%, #333a3d 50%);
	}
	.swatch.auto i + i + i {
		background: linear-gradient(90deg, #53676c 50%, #94b0b8 50%);
	}

	.switch {
		display: flex;
		align-items: flex-start;
		gap: 0.7rem;
		max-width: 30rem;
		cursor: pointer;
	}
	.switch input {
		margin-top: 0.2rem;
	}
</style>
