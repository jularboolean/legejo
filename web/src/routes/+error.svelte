<script lang="ts">
	// What is shown when a page cannot be loaded. Without the network that is
	// every page not on the device, so the way to the saved books is offered.
	import { page } from '$app/state';
	import { t } from '#lib/i18n';
</script>

<main>
	<h1>{page.status}</h1>
	<p>{page.error?.message}</p>
	<p>
		<!-- Whole loads, not the app's own navigation: that is what failed. -->
		<a href="/offline" data-sveltekit-reload>{t('offline.toSaved')}</a>
		&ensp;·&ensp;
		<a href="/" data-sveltekit-reload>{t('offline.retry')}</a>
	</p>
</main>

<style>
	main {
		max-width: 30rem;
		margin: 18vh auto 0;
		padding: 0 1.25rem;
		text-align: center;
	}
	h1 {
		margin: 0 0 0.5rem;
	}
	p {
		color: var(--muted);
	}
</style>
