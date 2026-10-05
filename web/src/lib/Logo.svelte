<script lang="ts">
	// Full-colour marks, so they keep their own palette (works on light and dark).
	import single from './assets/duva.svg?raw';
	import pair from './assets/legejo.svg?raw';

	let { size = 40, variant = 'single' }: { size?: number; variant?: 'single' | 'pair' } = $props();

	// Width over height, from each file's viewBox.
	const MARKS = {
		single: { svg: single, ratio: 225.65 / 341.36 },
		pair: { svg: pair, ratio: 321.825 / 341.68 }
	};
	const mark = $derived(MARKS[variant]);
</script>

<span class="logo" style:width={`${size * mark.ratio}px`} style:height={`${size}px`}>
	{@html mark.svg}
</span>

<style>
	.logo {
		display: inline-flex;
	}
	.logo :global(svg) {
		width: 100%;
		height: 100%;
	}
</style>
