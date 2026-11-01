// Getting a saved place on screen. A book that is shown at a place can end up
// on an earlier page: epub.js moves to the place before a slow device has laid
// the chapter out, and no further than the chapter is wide so far.

/** How many times a place is shown anew when it did not end up on screen. */
export const PLACE_TRIES = 4;

export type PlaceSteps = {
	/** Whether the place lies beyond what is on screen, in the same chapter. */
	beyond: () => boolean;
	/** One page on. */
	next: () => Promise<unknown>;
	/** Show the place anew. */
	again: () => Promise<unknown>;
	/** Give the chapter a moment to be laid out. */
	wait: () => Promise<unknown>;
};

export async function reachPlace({ beyond, next, again, wait }: PlaceSteps): Promise<void> {
	if (!beyond()) return;
	// Usually it is just over the page: see correctPage.
	await next();
	for (let i = 0; i < PLACE_TRIES && beyond(); i++) {
		await wait();
		await again();
		if (beyond()) await next();
	}
}
