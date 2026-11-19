// A time limit that counts only the time in which the page can draw.
//
// epub.js moves its work along on animation frames, which a browser stops in
// a tab that is not shown. Measured by the clock, a book opened in a
// background tab would run out of time before it had a chance; measured in
// frames, the time behind the scenes does not count.

/** The most one frame adds: the first frame after a pause brings the whole pause along. */
const LONGEST_FRAME_MS = 250;

/** The promise, or a rejection with Error('timeout') once `ms` of drawable time have passed. */
export function timeout<T>(promise: Promise<T>, ms: number): Promise<T> {
	return new Promise<T>((resolve, reject) => {
		let spent = 0;
		let last: number | null = null;
		let frame = 0;
		let done = false;
		const tick = (now: number) => {
			if (done) return;
			if (last !== null) spent += Math.min(Math.max(now - last, 0), LONGEST_FRAME_MS);
			last = now;
			if (spent >= ms) {
				done = true;
				reject(new Error('timeout'));
				return;
			}
			frame = requestAnimationFrame(tick);
		};
		frame = requestAnimationFrame(tick);
		promise.then(
			(value) => {
				done = true;
				cancelAnimationFrame(frame);
				resolve(value);
			},
			(err) => {
				done = true;
				cancelAnimationFrame(frame);
				reject(err);
			}
		);
	});
}
