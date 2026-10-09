// A swipe across the page on a phone: to the right it opens the menu, to the
// left it closes it. The decision is kept apart from the events so it can be
// tested; the layout feeds it the touch points.

export type Point = { x: number; y: number; time: number };

/** The strip at the left edge that belongs to the system (back, in a browser tab). */
export const EDGE_PX = 24;
const SWIPE_MIN_PX = 60;
const SWIPE_MAX_MS = 700;

/**
 * Whether a touch that begins at `point` on `target` can become a swipe for
 * the menu: not at the system's edge, not in a text field, and not in
 * something that scrolls sideways on its own.
 */
export function swipeMayStart(point: Point, target: Element | null): boolean {
	if (point.x < EDGE_PX) return false;
	if (target?.closest?.('input, textarea, select, [contenteditable="true"]')) return false;
	for (let el = target; el && el !== el.ownerDocument.body; el = el.parentElement) {
		if (el.scrollWidth > el.clientWidth + 1) {
			const overflow = getComputedStyle(el).overflowX;
			if (overflow === 'auto' || overflow === 'scroll') return false;
		}
	}
	return true;
}

/** 'open', 'close' or null, for a touch that began at `start` and ended at `end`. */
export function swipeAction(start: Point, end: Point, menuOpen: boolean): 'open' | 'close' | null {
	if (end.time - start.time > SWIPE_MAX_MS) return null;
	const dx = end.x - start.x;
	const dy = end.y - start.y;
	if (Math.abs(dx) < SWIPE_MIN_PX || Math.abs(dx) < Math.abs(dy) * 1.5) return null;
	if (dx > 0) return menuOpen ? null : 'open';
	return menuOpen ? 'close' : null;
}
