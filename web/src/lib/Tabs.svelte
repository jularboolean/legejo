<script lang="ts">
	// The parts of a long page as tabs. Each is a link, so that a part has
	// an address of its own and the browser's back button works.
	type Tab = { href: string; label: string; active: boolean; dot?: boolean };

	let { tabs, label }: { tabs: Tab[]; label: string } = $props();
</script>

<nav class="tabs" aria-label={label}>
	{#each tabs as tab (tab.href)}
		<a href={tab.href} class:active={tab.active} aria-current={tab.active ? 'page' : undefined} data-sveltekit-noscroll>
			{tab.label}
			{#if tab.dot}<span class="dot"></span>{/if}
		</a>
	{/each}
</nav>

<style>
	.tabs {
		display: flex;
		gap: 1.5rem;
		margin: 0 0 1.75rem;
		border-bottom: 1px solid var(--border);
		overflow-x: auto;
	}
	a {
		position: relative;
		padding: 0.5rem 0;
		margin-bottom: -1px;
		color: var(--muted);
		text-decoration: none;
		white-space: nowrap;
		border-bottom: 2px solid transparent;
	}
	a:hover {
		color: var(--fg);
	}
	a.active {
		color: var(--fg);
		font-weight: 600;
		border-bottom-color: var(--accent);
	}
	.dot {
		display: inline-block;
		width: 0.45rem;
		height: 0.45rem;
		margin-left: 0.3rem;
		border-radius: 50%;
		background: var(--danger);
		vertical-align: middle;
	}
</style>
