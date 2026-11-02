<script lang="ts">
	// The cover of a book that has none of its own: the pigeons above the
	// title, as on the manual's cover. It takes the size of what it is put in.
	import doves from '#lib/assets/legejo.svg?url';

	let { title, author = null, format = null }: { title: string; author?: string | null; format?: string | null } = $props();

	const BANDS = ['#84467a', '#4e6469', '#4f6f52', '#3f6f75', '#5f5a8c', '#44608a'];

	const band = $derived.by(() => {
		let h = 0;
		for (const c of title) h = (h * 31 + c.charCodeAt(0)) | 0;
		return BANDS[Math.abs(h) % BANDS.length];
	});
	const length = $derived(title.length > 70 ? 'long' : title.length > 28 ? 'medium' : 'short');
</script>

<span class="book-cover">
	<span class="inner">
		<span class="format">{#if format && format !== 'epub'}{format.toUpperCase()}{/if}</span>
		<img src={doves} alt="" loading="lazy" />
		<span class="band {length}" style:background={band}><span class="title">{title}</span></span>
		{#if author}<span class="author">{author}</span>{/if}
	</span>
</span>

<style>
	.book-cover {
		display: block;
		width: 100%;
		height: 100%;
		container-type: size;
		/* Paper, whatever the theme: it is a book lying on the page. */
		background: #fbfaf6;
		color: #1d2224;
	}
	.inner {
		display: flex;
		flex-direction: column;
		height: 100%;
		padding: 7cqw 7cqw 6cqw;
		box-sizing: border-box;
		overflow: hidden;
		text-align: left;
	}
	.format {
		flex: none;
		min-height: 1.3em;
		padding-top: 1.6cqw;
		border-top: max(1px, 0.7cqw) solid currentColor;
		font-size: 5.8cqw;
		font-weight: 600;
		letter-spacing: 0.08em;
		line-height: 1.3;
		text-align: right;
	}
	img {
		flex: 1 1 0;
		min-height: 0;
		align-self: center;
		width: 58%;
		object-fit: contain;
		margin: 2cqw 0 5cqw;
	}
	.band {
		flex: none;
		padding: 5cqw 6cqw 6cqw;
		color: #fff;
		font-family: 'Literata Variable', Georgia, serif;
		font-size: 11cqw;
		line-height: 1.12;
	}
	.band.medium {
		font-size: 9cqw;
	}
	.band.long {
		font-size: 7.2cqw;
	}
	.title {
		display: -webkit-box;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 5;
		line-clamp: 5;
		overflow: hidden;
		overflow-wrap: anywhere;
	}
	.author {
		flex: none;
		margin-top: 3.5cqw;
		font-family: 'Literata Variable', Georgia, serif;
		font-style: italic;
		font-size: 6.6cqw;
		line-height: 1.2;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	/* Too small to read: the pigeons and the colour say it. */
	@container (max-width: 96px) {
		.title,
		.author,
		.format {
			display: none;
		}
		img {
			width: 100%;
		}
		.band {
			padding: 0;
			height: 12cqh;
		}
	}
</style>
