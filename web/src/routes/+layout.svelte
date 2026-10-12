<script lang="ts">
	import '../app.css';
	import { onNavigate } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount, type Snippet } from 'svelte';
	import { getLocale } from '#lib/i18n';
	import { reducedMotion } from '#lib/motion';
	import { pageTitle } from '#lib/pageTitle';
	import icon from '#lib/assets/duva.png';

	// The root layout only carries global styles. The catalog chrome (sidebar,
	// shelves) lives in the (app) group, so full-screen routes such as the
	// reader can opt out of it by living outside that group.
	let { children }: { children: Snippet } = $props();

	// There is a page to show: what app.html put up while the app started goes.
	onMount(() => document.getElementById('starting')?.remove());

	// One page fades into the next, where the browser can do that. Not within
	// a page (a search or a filter in the address), where it would flicker.
	onNavigate((navigation) => {
		if (!document.startViewTransition || reducedMotion()) return;
		if (navigation.from?.route.id === navigation.to?.route.id) return;
		return new Promise((resolve) => {
			const transition = document.startViewTransition(async () => {
				resolve();
				await navigation.complete;
			});
			// Skipped by the browser (a hidden tab, another navigation on its
			// heels): the page changes all the same.
			transition.ready.catch(() => {});
			transition.finished.catch(() => {});
		});
	});

	// The one place the tab title is set (see pageTitle.ts).
	const title = $derived(pageTitle(page.route.id, page.data));

	// The colour of the browser chrome, and of the status bar when Legejo is
	// installed as an app. The reader sets its own, after the page's theme.
	const reading = $derived(page.route.id === '/books/[id]/read');

	// Screen readers and hyphenation follow the chosen UI language.
	$effect(() => {
		document.documentElement.lang = getLocale();
	});
</script>

<svelte:head>
	<title>{title}</title>
	<link rel="icon" type="image/png" href={icon} />
	{#if !reading}
		<meta name="theme-color" content="#f3f5f5" media="(prefers-color-scheme: light)" />
		<meta name="theme-color" content="#15181a" media="(prefers-color-scheme: dark)" />
	{/if}
</svelte:head>

{@render children()}
