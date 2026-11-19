import { afterEach, describe, expect, it, vi } from 'vitest';
import { timeout } from './timeout';

/** Animation frames run by hand: `frames(n, ms)` draws n frames ms apart. */
function fakeFrames() {
	let now = 0;
	let waiting: ((t: number) => void)[] = [];
	vi.stubGlobal('requestAnimationFrame', (cb: (t: number) => void) => waiting.push(cb));
	vi.stubGlobal('cancelAnimationFrame', () => {});
	return {
		async frames(n: number, ms: number) {
			for (let i = 0; i < n; i++) {
				now += ms;
				const run = waiting;
				waiting = [];
				for (const cb of run) cb(now);
				await Promise.resolve();
			}
		},
		/** Time passes without a frame, as in a tab that is not shown. */
		hidden(ms: number) {
			now += ms;
		}
	};
}

afterEach(() => vi.unstubAllGlobals());

describe('timeout', () => {
	it('runs out after the time the page could draw', async () => {
		const clock = fakeFrames();
		const result = timeout(new Promise(() => {}), 1000).catch((e: Error) => e.message);
		await clock.frames(12, 100);
		expect(await result).toBe('timeout');
	});

	it('does not count the time a tab is not shown', async () => {
		const clock = fakeFrames();
		let settled = '';
		let finish: (v: string) => void = () => {};
		timeout(new Promise<string>((r) => (finish = r)), 1000).then(
			(v) => (settled = v),
			(e: Error) => (settled = e.message)
		);
		await clock.frames(5, 100);
		clock.hidden(60_000);
		// The first frame after the pause brings a little of it along, not all.
		await clock.frames(3, 100);
		await Promise.resolve();
		expect(settled).toBe('');
		finish('opened');
		await Promise.resolve();
		await Promise.resolve();
		expect(settled).toBe('opened');
	});

	it('passes a failure through', async () => {
		fakeFrames();
		await expect(timeout(Promise.reject(new Error('broken')), 1000)).rejects.toThrow('broken');
	});
});
