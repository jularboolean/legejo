<script lang="ts">
	import '../app.css';
	import { page } from '$app/state';
	import type { Snippet } from 'svelte';
	import { getLocale } from '#lib/i18n';
	import { pageTitle } from '#lib/pageTitle';
	import icon from '#lib/assets/duva.png';

	// The root layout only carries global styles. The catalog chrome (sidebar,
	// shelves) lives in the (app) group, so full-screen routes such as the
	// reader can opt out of it by living outside that group.
	let { children }: { children: Snippet } = $props();

	// The one place the tab title is set (see pageTitle.ts).
	const title = $derived(pageTitle(page.route.id, page.data));

	// Screen readers and hyphenation follow the chosen UI language.
	$effect(() => {
		document.documentElement.lang = getLocale();
	});
</script>

<svelte:head>
	<title>{title}</title>
	<link rel="icon" type="image/png" href={icon} />
	<link rel="apple-touch-icon" href={icon} />
</svelte:head>

{@render children()}
