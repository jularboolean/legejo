import { describe, expect, it } from 'vitest';
import { swipeAction, swipeMayStart } from '#lib/drawerSwipe';

const at = (x: number, y: number, time = 0) => ({ x, y, time });

describe('a swipe for the menu', () => {
	it('opens the menu on a swipe to the right, and closes it on one to the left', () => {
		expect(swipeAction(at(100, 300), at(200, 310, 300), false)).toBe('open');
		expect(swipeAction(at(200, 300), at(100, 310, 300), true)).toBe('close');
	});

	it('does nothing in the direction the menu already is', () => {
		expect(swipeAction(at(100, 300), at(200, 300, 300), true)).toBeNull();
		expect(swipeAction(at(200, 300), at(100, 300, 300), false)).toBeNull();
	});

	it('is not a scroll, a tap or a slow drag', () => {
		expect(swipeAction(at(100, 100), at(180, 300, 300), false)).toBeNull();
		expect(swipeAction(at(100, 100), at(130, 100, 100), false)).toBeNull();
		expect(swipeAction(at(100, 100), at(300, 100, 2000), false)).toBeNull();
	});

	it('leaves the edge of the screen to the system, and text fields alone', () => {
		// The tests run without a DOM; this is as much of an element as is looked at.
		const element = (field: boolean) =>
			({
				closest: () => (field ? {} : null),
				scrollWidth: 0,
				clientWidth: 0,
				parentElement: null,
				ownerDocument: { body: null }
			}) as unknown as Element;
		expect(swipeMayStart(at(10, 300), element(false))).toBe(false);
		expect(swipeMayStart(at(100, 300), element(true))).toBe(false);
		expect(swipeMayStart(at(100, 300), element(false))).toBe(true);
	});
});
