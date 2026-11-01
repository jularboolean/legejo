<script lang="ts">
	// Full-colour marks, so they keep their own palette (works on light and dark).
	import single from './assets/duva.svg?raw';
	import pair from './assets/legejo.svg?raw';
	import doctor from './assets/zamenhof.jpg?url';

	let {
		size = 40,
		variant = 'single',
		portrait = false
	}: { size?: number; variant?: 'single' | 'pair'; portrait?: boolean } = $props();

	// Width over height, from each file's viewBox.
	const MARKS = {
		single: { svg: single, ratio: 225.65 / 341.36 },
		pair: { svg: pair, ratio: 321.825 / 341.68 }
	};
	const mark = $derived(MARKS[variant]);
</script>

<span class="logo" style:width={`${size * mark.ratio}px`} style:height={`${size}px`}>
	{#if portrait}
		<img class="portrait" src={doctor} alt="" />
	{:else}
		{@html mark.svg}
	{/if}
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
