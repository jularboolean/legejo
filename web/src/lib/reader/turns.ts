/**
 * Page turns waiting behind a turn in progress. At most one turn waits: a new
 * turn the same way replaces the waiting one, and a turn the other way cancels
 * it (and is itself dropped), so taps made during a slow turn never pile up
 * into a run of turns that goes on after the input has stopped.
 */
export type TurnGate = (forward: boolean, run: () => Promise<void>) => Promise<void>;

/**
 * `enqueue` runs tasks one at a time, in order (the engine's render queue).
 * A cancelled turn still occupies its place in that queue, but does nothing
 * when it is reached.
 */
export function createTurnGate(enqueue: (task: () => Promise<void>) => Promise<void>): TurnGate {
	let waiting: { forward: boolean; cancelled: boolean } | null = null;

	return (forward, run) => {
		if (waiting) {
			const reversed = waiting.forward !== forward;
			waiting.cancelled = true;
			waiting = null;
			if (reversed) return Promise.resolve();
		}
		const entry = { forward, cancelled: false };
		waiting = entry;
		return enqueue(async () => {
			if (waiting === entry) waiting = null;
			if (entry.cancelled) return;
			await run();
		});
	};
}

/**
 * Whether the page runs in WebKit: Safari, and every browser on iOS and
 * iPadOS. Chromium-based browsers also name AppleWebKit in their user agent,
 * followed by a Chrome or Edg token; the iOS browsers (CriOS, FxiOS, EdgiOS)
 * do not carry those tokens.
 */
export function isWebKit(userAgent: string): boolean {
	return /AppleWebKit\//.test(userAgent) && !/(Chrome|Chromium|Edg|OPR)\//.test(userAgent);
}
