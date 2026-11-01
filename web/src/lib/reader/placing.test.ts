import { describe, expect, it, vi } from 'vitest';
import { PLACE_TRIES, reachPlace } from './placing';

/** A book on `page`, with the place on page `target`; showing it lands on `lands`. */
function book(page: number, target: number, lands: () => number) {
	const state = { page };
	return {
		state,
		steps: {
			beyond: () => state.page < target,
			next: vi.fn(async () => void state.page++),
			again: vi.fn(async () => void (state.page = lands())),
			wait: vi.fn(async () => {})
		}
	};
}

describe('reachPlace', () => {
	it('does nothing when the place is on screen', async () => {
		const { steps } = book(5, 5, () => 5);
		await reachPlace(steps);
		expect(steps.next).not.toHaveBeenCalled();
		expect(steps.again).not.toHaveBeenCalled();
	});

	it('turns one page when the place is just over it', async () => {
		const { steps } = book(4, 5, () => 5);
		await reachPlace(steps);
		expect(steps.next).toHaveBeenCalledTimes(1);
		expect(steps.again).not.toHaveBeenCalled();
	});

	it('shows the place anew when the book opened far before it', async () => {
		const { state, steps } = book(1, 20, () => 20);
		await reachPlace(steps);
		expect(state.page).toBe(20);
		expect(steps.again).toHaveBeenCalledTimes(1);
	});

	it('waits for a chapter that takes its time', async () => {
		let shown = 0;
		const { state, steps } = book(1, 20, () => (++shown < 3 ? 1 : 20));
		await reachPlace(steps);
		expect(state.page).toBe(20);
		expect(steps.wait).toHaveBeenCalledTimes(3);
	});

	it('gives up in the end, and stays where it came to', async () => {
		const { steps } = book(1, 20, () => 1);
		await reachPlace(steps);
		expect(steps.again).toHaveBeenCalledTimes(PLACE_TRIES);
	});
});
