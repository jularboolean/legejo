<script lang="ts">
	// The reader's other books by the same author, under a book's details.
	// Found by the library's search on the surname and kept to the books
	// whose author is the same person, however the name is written.
	import { t } from '#lib/i18n';
	import BookCover from '#lib/library/BookCover.svelte';
	import { splitAuthors, type Person } from '#lib/library/authors';
	import type { LibraryBook } from '#lib/types';

	type Props = { bookId: number; author: string | null };
	let { bookId, author }: Props = $props();

	/** Books shown per author; the rest are behind the link to all of them. */
	const SHOWN = 8;

	type Found = { person: Person; books: LibraryBook[]; more: boolean };
	let found = $state<Found[]>([]);

	$effect(() => {
		const people = splitAuthors(author).slice(0, 2);
		const id = bookId;
		let gone = false;
		found = [];
		Promise.all(
			people.map(async (person) => {
				const surname = person.sort.split(' ')[0] ?? person.name;
				const res = await fetch(`/api/books?q=${encodeURIComponent(surname)}`).catch(() => null);
				if (!res?.ok) return null;
				const list: LibraryBook[] = await res.json().catch(() => []);
				const books = list.filter((b) => b.id !== id && splitAuthors(b.author).some((p) => p.key === person.key));
				return books.length ? { person, books: books.slice(0, SHOWN), more: books.length > SHOWN } : null;
			})
		).then((all) => {
			if (!gone) found = all.filter((f): f is Found => f !== null);
		});
		return () => {
			gone = true;
		};
	});

	function cover(b: LibraryBook): string {
		return `/api/books/${b.id}/cover?size=thumb&v=${encodeURIComponent(b.updated_at ?? b.created_at)}`;
	}
</script>

{#each found as f (f.person.key)}
	<section class="more-by">
		<h2>
			{t('book.moreBy', { name: f.person.name })}
			{#if f.more}<a href={`/?author=${encodeURIComponent(f.person.key)}`}>{t('book.moreByAll')}</a>{/if}
		</h2>
		<ul>
			{#each f.books as b (b.id)}
				<li>
					<a href={`/books/${b.id}`} title={b.title}>
						{#if b.has_cover}
							<img src={cover(b)} alt="" loading="lazy" />
						{:else}
							<span class="plain"><BookCover title={b.title} author={b.author} format={b.format} /></span>
						{/if}
						<span class="title">{b.title}</span>
					</a>
				</li>
			{/each}
		</ul>
	</section>
{/each}

<style>
	.more-by {
		margin-top: 2rem;
	}
	h2 {
		display: flex;
		align-items: baseline;
		gap: 0.75rem;
		font-size: 1rem;
		margin: 0 0 0.75rem;
	}
	h2 a {
		font-size: 0.85rem;
		font-weight: normal;
		color: var(--accent);
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0 0 0.25rem;
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: 6.5rem;
		gap: 0.9rem;
		overflow-x: auto;
	}
	li a {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		color: inherit;
		text-decoration: none;
	}
	img,
	.plain {
		width: 6.5rem;
		aspect-ratio: 2 / 3;
		object-fit: cover;
		border-radius: 4px;
		box-shadow: 0 1px 4px rgba(0, 0, 0, 0.15);
		background: var(--card);
	}
	.plain {
		display: block;
		overflow: hidden;
		font-size: 0.6rem;
	}
	.title {
		font-size: 0.78rem;
		line-height: 1.3;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	li a:hover .title {
		text-decoration: underline;
	}
</style>
