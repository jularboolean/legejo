<script lang="ts" module>
	import raw from './assets/duva.svg?raw';

	// The logo's own paths, body first; drawn here so a rating can show the
	// dove in colour or as a pale silhouette. Each path keeps the place the
	// drawing gives it (its own transform), and the picture keeps its frame.
	const PATHS = [...raw.matchAll(/<path\b[^>]*>/g)].map(([tag]) => ({
		d: /\sd="([^"]+)"/.exec(tag)?.[1] ?? '',
		// As an attribute (fill="#5A7075") or in a style (fill:#5A7075).
		fill: (/fill(?::|=")\s*(#[0-9a-fA-F]{6})/.exec(tag)?.[1] ?? '#000').toLowerCase(),
		transform: /\stransform="([^"]+)"/.exec(tag)?.[1]
	}));
	const VIEW_BOX = /viewBox="([^"]+)"/.exec(raw)?.[1] ?? '0 0 225 341';
	const [, , WIDTH, HEIGHT] = VIEW_BOX.split(/\s+/).map(Number);
	/** The body's colour, which a filled dove takes from the page's accent. */
	const BODY = '#5a7075';
	const RATIO = WIDTH / HEIGHT;
</script>

<script lang="ts">
	/** `size` is the height; the dove is about two thirds as wide. */
	let { size = 20, filled = false }: { size?: number; filled?: boolean } = $props();
</script>

<svg class="dove" class:empty={!filled} width={size * RATIO} height={size} viewBox={VIEW_BOX} aria-hidden="true">
	<g>
		{#each PATHS as path (path.d)}
			<path
				d={path.d}
				transform={path.transform}
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
	/* On a dark page the pale silhouette needs more of itself to be seen. */
	:global(:root[data-design='dark']) .empty {
		opacity: 0.55;
	}
	@media (prefers-color-scheme: dark) {
		:global(:root:not([data-design])) .empty {
			opacity: 0.55;
		}
	}
</style>
