<script lang="ts">
	let {
		open,
		title,
		message = '',
		confirmLabel,
		cancelLabel,
		danger = false,
		onconfirm,
		oncancel
	}: {
		open: boolean;
		title: string;
		message?: string;
		confirmLabel: string;
		cancelLabel: string;
		danger?: boolean;
		onconfirm: () => void;
		oncancel: () => void;
	} = $props();

	let cancelBtn = $state<HTMLButtonElement | null>(null);

	$effect(() => {
		if (open) cancelBtn?.focus();
	});

	function onkeydown(e: KeyboardEvent) {
		if (open && e.key === 'Escape') {
			e.preventDefault();
			oncancel();
		}
	}
</script>

<svelte:window {onkeydown} />

{#if open}
	<div class="backdrop" onclick={oncancel} role="presentation">
		<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
		<div
			class="dialog"
			role="alertdialog"
			aria-modal="true"
			aria-label={title}
			tabindex="-1"
			onclick={(e) => e.stopPropagation()}
		>
			<h2>{title}</h2>
			{#if message}<p class="message">{message}</p>{/if}
			<div class="actions">
				<button type="button" class="ghost" bind:this={cancelBtn} onclick={oncancel}>
					{cancelLabel}
				</button>
				<button type="button" class:danger onclick={onconfirm}>
					{confirmLabel}
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		background: rgba(20, 16, 10, 0.45);
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 1rem;
		z-index: 100;
	}
	.dialog {
		background: var(--card);
		border: 1px solid var(--border);
		border-radius: 10px;
		box-shadow: var(--shadow);
		padding: 1.5rem;
		width: 100%;
		max-width: 24rem;
	}
	h2 {
		margin: 0 0 0.5rem;
		font-size: 1.15rem;
	}
	.message {
		margin: 0;
		color: var(--muted);
		font-size: 0.92rem;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.6rem;
		margin-top: 1.5rem;
	}
	.danger {
		background: var(--danger);
		color: var(--bg);
	}
</style>
