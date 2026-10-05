// Numbers for the reading page, all computed from the library listing: the
// whole library arrives in one request with progress, last read time,
// added date, rating, language and author per book.

import { primaryLanguage, readStatus } from './library';
import { splitAuthors } from './library/authors';
import type { LibraryBook } from './types';

export type Month = { key: string; date: Date; count: number };

/** The last `n` calendar months, oldest first, each with a count. */
export function perMonth(dates: (string | null | undefined)[], n = 12, now = new Date()): Month[] {
	const months: Month[] = [];
	for (let i = n - 1; i >= 0; i--) {
		const d = new Date(now.getFullYear(), now.getMonth() - i, 1);
		months.push({ key: `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`, date: d, count: 0 });
	}
	const index = new Map(months.map((m, i) => [m.key, i]));
	for (const at of dates) {
		if (!at) continue;
		const d = new Date(at);
		const i = index.get(`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`);
		if (i !== undefined) months[i].count++;
	}
	return months;
}

export type Tally = { label: string; count: number };

function top(counts: Map<string, number>, n: number): Tally[] {
	return [...counts.entries()]
		.map(([label, count]) => ({ label, count }))
		.sort((a, b) => b.count - a.count || a.label.localeCompare(b.label))
		.slice(0, n);
}

export function readingStats(books: LibraryBook[]) {
	const finished = books.filter((b) => readStatus(b) === 'finished');
	const reading = books
		.filter((b) => readStatus(b) === 'reading')
		.sort((a, b) => (b.last_read_at ?? '').localeCompare(a.last_read_at ?? ''));
	const wanted = books
		.filter((b) => b.want_to_read)
		.sort((a, b) => (b.wanted_at ?? '').localeCompare(a.wanted_at ?? ''));
	const recentlyFinished = [...finished].sort((a, b) => (b.last_read_at ?? '').localeCompare(a.last_read_at ?? ''));

	const authorsRead = new Map<string, number>();
	for (const b of finished) for (const p of splitAuthors(b.author)) authorsRead.set(p.name, (authorsRead.get(p.name) ?? 0) + 1);
	const authorsOwned = new Map<string, number>();
	for (const b of books) for (const p of splitAuthors(b.author)) authorsOwned.set(p.name, (authorsOwned.get(p.name) ?? 0) + 1);
	const languages = new Map<string, number>();
	for (const b of books) {
		const code = primaryLanguage(b.language);
		if (code) languages.set(code, (languages.get(code) ?? 0) + 1);
	}

	const rated = books.filter((b) => b.rating);
	const ratings = [1, 2, 3, 4, 5].map((r) => rated.filter((b) => b.rating === r).length);
	const thisYear = String(new Date().getFullYear());

	return {
		total: books.length,
		finished,
		reading,
		wanted,
		recentlyFinished,
		unread: books.length - finished.length - reading.length,
		finishedThisYear: finished.filter((b) => (b.last_read_at ?? '').startsWith(thisYear)).length,
		finishedPerMonth: perMonth(finished.map((b) => b.last_read_at)),
		addedPerMonth: perMonth(books.map((b) => b.created_at)),
		topAuthorsRead: top(authorsRead, 5),
		topAuthorsOwned: top(authorsOwned, 5),
		languages: top(languages, 6),
		ratings,
		ratedCount: rated.length,
		averageRating: rated.length ? rated.reduce((s, b) => s + (b.rating ?? 0), 0) / rated.length : null
	};
}
