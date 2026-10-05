<script lang="ts" module>
	import raw from './assets/duva.svg?raw';

	// The logo's own paths, body first; drawn here so a rating can show the
	// dove in colour or as a pale silhouette.
	const PATHS = [...raw.matchAll(/<path\b[^>]*>/g)].map(([tag]) => ({
		d: /\sd="([^"]+)"/.exec(tag)?.[1] ?? '',
		fill: /fill:(#[0-9a-fA-F]{6})/.exec(tag)?.[1] ?? '#000'
	}));
	const OFFSET = /<g\b[^>]*transform="([^"]+)"/.exec(raw)?.[1] ?? '';
	const BODY = '#53676c';
	const RATIO = 225.65 / 341.36;
</script>

<script lang="ts">
	/** `size` is the height; the dove is about two thirds as wide. */
	let { size = 20, filled = false }: { size?: number; filled?: boolean } = $props();
</script>

<svg class="dove" class:empty={!filled} width={size * RATIO} height={size} viewBox="0 0 225.65 341.36" aria-hidden="true">
	<g transform={OFFSET}>
		{#each PATHS as path (path.d)}
			<path
				d={path.d}
				fill-rule="evenodd"
				fill={!filled ? 'currentColor' : path.fill === BODY ? 'var(--accent)' : path.fill}
			/>
		{/each}
	</g>
</svg>

<style>
	.dove {
		display: block;
		flex-shrink: 0;
		overflow: visible;
	}
	/* Every part in one flat tone, so the empty dove has exactly the coloured
	   one's silhouette; the opacity sits on the whole so overlaps don't show. */
	.empty {
		opacity: 0.3;
	}
</style>
