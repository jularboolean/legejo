<script lang="ts">
	import { page } from '$app/state';
	import Reader from '#lib/reader/Reader.svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	// ?at=<highlight> opens the book there; ?from=notes leads back to the notes;
	// ?q=<words> opens the search in the book with them (from the search page).
	const at = $derived(Number(page.url.searchParams.get('at')) || null);
	const find = $derived(page.url.searchParams.get('q')?.trim() || null);
	const back = $derived(page.url.searchParams.get('from') === 'notes' ? '/notes' : null);
</script>

<!-- Keyed, so going straight from one book to another starts a fresh reader. -->
{#key data.book.id}
	<Reader book={data.book} {at} {back} {find} />
{/key}
