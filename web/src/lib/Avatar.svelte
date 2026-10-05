<script lang="ts">
	import defaultAvatar from './assets/profile.png';

	let {
		userId,
		hasAvatar,
		size = 28,
		alt = '',
		version = 0
	}: {
		userId: number;
		hasAvatar: boolean;
		size?: number;
		alt?: string;
		/** Bump to bypass the browser cache after an upload. */
		version?: number;
	} = $props();

	const src = $derived(hasAvatar ? `/api/users/${userId}/avatar?v=${version}` : defaultAvatar);
</script>

<img class="avatar" {src} {alt} style:width={`${size}px`} style:height={`${size}px`} />

<style>
	.avatar {
		border-radius: 50%;
		object-fit: cover;
		flex-shrink: 0;
		border: 1px solid var(--border);
		background: var(--card);
	}
</style>
