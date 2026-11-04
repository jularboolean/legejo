// A visit: the book is opened to look something up (a search hit, a
// highlight, a passage in a finished book), not to read on. The reading
// position is then left where it was until the reader says otherwise, or
// reads on for long enough that it is clearly reading.

import type { Book } from '#lib/types';

/** Pages turned forward during a visit before it counts as reading. */
export const VISIT_TURNS = 10;

/** Finished: marked as read by hand, or read to the end. */
export function isFinished(book: Pick<Book, 'read_at' | 'progress_percent'>): boolean {
	return !!book.read_at || (book.progress_percent ?? 0) >= 0.99;
}

/**
 * Whether the reader opens as a visit. Asked to read again, it never does;
 * otherwise it does when it opens at a highlight or a search, or the book is
 * already finished.
 */
export function opensAsVisit(
	book: Pick<Book, 'read_at' | 'progress_percent'>,
	opened: { at?: number | null; find?: string | null; reread?: boolean }
): boolean {
	if (opened.reread) return false;
	return opened.at != null || !!opened.find || isFinished(book);
}

/**
 * Whether the reader opens at the start, whatever the position says: when
 * reading again, and when looking through a finished book, whose position is
 * its end.
 */
export function opensAtStart(
	book: Pick<Book, 'read_at' | 'progress_percent'>,
	opened: { at?: number | null; find?: string | null; reread?: boolean }
): boolean {
	return !!opened.reread || (isFinished(book) && opened.at == null && !opened.find);
}

/** Counts pages turned forward in a row; true once the visit has become reading. */
export function visitCounter(turns = VISIT_TURNS) {
	let count = 0;
	return {
		/** A page turned; going back starts the count again. */
		turned(forward: boolean): boolean {
			count = forward ? count + 1 : 0;
			return count >= turns;
		},
		/** A jump elsewhere in the book: what was read in a row ends there. */
		reset() {
			count = 0;
		}
	};
}
