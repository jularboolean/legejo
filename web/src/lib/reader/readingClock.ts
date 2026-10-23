// Time spent reading in the web reader: counted while the page is in view
// and the reader has done something lately (turned a page, scrolled,
// touched or typed), or while the book is being read aloud. What is counted
// is kept on the device until the server has it, and sent about once a
// minute and when the page is left (see readingtime.rs).

/** Without anything from the reader for this long, the time stops counting. */
export const IDLE_MS = 3 * 60_000;
/** How often the clock looks. */
const TICK_MS = 5_000;
/** Counted time is sent once there is this much of it. */
const SEND_SECONDS = 60;
/** The server takes at most this much in one report. */
const MOST_SECONDS = 6 * 3600;

const PREFIX = 'legejo.reader.time.';

/**
 * The seconds of a stretch of `elapsed` milliseconds that count as reading:
 * all of it while the reader was active, none once they have been idle too
 * long or the page was out of view. A stretch longer than a tick (the device
 * slept) counts for one tick at most.
 */
export function countable(elapsedMs: number, sinceActivityMs: number, visible: boolean, speaking: boolean): number {
	if (!visible || elapsedMs <= 0) return 0;
	if (!speaking && sinceActivityMs > IDLE_MS) return 0;
	return Math.min(elapsedMs, TICK_MS) / 1000;
}

function readKept(key: string): number {
	try {
		const v = Number(localStorage.getItem(key));
		return Number.isFinite(v) && v > 0 ? v : 0;
	} catch {
		return 0;
	}
}

function writeKept(key: string, seconds: number) {
	try {
		if (seconds >= 1) localStorage.setItem(key, String(seconds));
		else localStorage.removeItem(key);
	} catch {
		// Not kept across reloads; sent while the page is open all the same.
	}
}

/** Report time to the server; true when it has it, or will never take it. */
async function report(bookId: string, seconds: number, keepalive: boolean): Promise<boolean> {
	try {
		const res = await fetch(`/api/books/${bookId}/reading-time`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ seconds }),
			keepalive
		});
		// An older server (404), or a book no longer there: nothing to keep.
		return res.ok || res.status === 404 || res.status === 422;
	} catch {
		return false;
	}
}

export type ReadingClock = {
	/** The reader did something. */
	touch(): void;
	/** Send what is counted now; safe while the page is being hidden. */
	flush(): Promise<void>;
	stop(): void;
};

/** What the reader is doing: reading (counted while active), listening to the book (counted), or neither. */
export type ClockState = 'reading' | 'listening' | 'away';

export function startReadingClock(bookId: number | string, state: () => ClockState): ReadingClock {
	const id = String(bookId);
	const key = PREFIX + id;
	let lastActivity = Date.now();
	let lastTick = Date.now();
	let sending = false;

	async function send(keepalive: boolean) {
		const seconds = Math.min(Math.floor(readKept(key)), MOST_SECONDS);
		if (seconds < 1 || sending) return;
		sending = true;
		try {
			if (await report(id, seconds, keepalive)) writeKept(key, readKept(key) - seconds);
		} finally {
			sending = false;
		}
	}

	const timer = setInterval(() => {
		const now = Date.now();
		const doing = state();
		const visible = doing !== 'away' && document.visibilityState === 'visible';
		const add = countable(now - lastTick, now - lastActivity, visible, doing === 'listening');
		lastTick = now;
		if (add <= 0) return;
		const kept = readKept(key) + add;
		writeKept(key, kept);
		if (kept >= SEND_SECONDS) void send(false);
	}, TICK_MS);

	return {
		touch() {
			lastActivity = Date.now();
		},
		flush() {
			return send(true);
		},
		stop() {
			clearInterval(timer);
		}
	};
}

/** Send the time kept on the device for any book, as after reading without the network. */
export async function sendUnsentTime(): Promise<void> {
	const keys: string[] = [];
	try {
		for (let i = 0; i < localStorage.length; i++) {
			const key = localStorage.key(i);
			if (key?.startsWith(PREFIX)) keys.push(key);
		}
	} catch {
		return;
	}
	for (const key of keys) {
		const seconds = Math.min(Math.floor(readKept(key)), MOST_SECONDS);
		if (seconds < 1) continue;
		if (!(await report(key.slice(PREFIX.length), seconds, false))) return;
		writeKept(key, readKept(key) - seconds);
	}
}
