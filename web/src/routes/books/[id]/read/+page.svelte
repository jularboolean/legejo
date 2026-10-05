<script lang="ts">
	import Reader from '#lib/reader/Reader.svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();
</script>

<!-- Keyed, so going straight from one book to another starts a fresh reader. -->
{#key data.book.id}
	<Reader book={data.book} />
{/key}
