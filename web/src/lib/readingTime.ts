// How long a stretch of text takes to read, at the pace the server gives
// for the reader (see `ReadingTime`), said as roughly as an estimate is.

import { formatLength } from './audio';

/** Minutes for `chars` characters at `pace` characters a minute. */
export function minutesFor(chars: number, pace: number): number {
	return pace > 0 && chars > 0 ? chars / pace : 0;
}

/**
 * An estimate as it is said: to the minute under ten minutes, to five
 * minutes under ten hours, to a quarter of an hour beyond. Null for
 * nothing left to read.
 */
export function roughly(minutes: number): string | null {
	if (!(minutes > 0)) return null;
	const step = minutes < 10 ? 1 : minutes < 600 ? 5 : 15;
	const rounded = Math.max(1, Math.round(minutes / step) * step);
	return formatLength(rounded * 60);
}
