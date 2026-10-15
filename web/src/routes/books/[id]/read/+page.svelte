<script lang="ts">
	import { page } from '$app/state';
	import Reader from '#lib/reader/Reader.svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	// ?at=<highlight> opens the book there; ?from=notes leads back to the notes.
	const at = $derived(Number(page.url.searchParams.get('at')) || null);
	const back = $derived(page.url.searchParams.get('from') === 'notes' ? '/notes' : null);
</script>

<!-- Keyed, so going straight from one book to another starts a fresh reader. -->
{#key data.book.id}
	<Reader book={data.book} {at} {back} />
{/key}
