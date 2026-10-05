// Reading position: the server when it has the progress API, otherwise (and
// always as a backup) localStorage. Nothing here surfaces errors to the reader.

export type Progress = {
	cfi: string;
	/** Overall progress, 0–1. */
	percent: number;
};

/**
 * Where to open the book. `cfi` is null when only a percentage is known, which
 * is the case when the book was last read on a Kobo.
 */
export type SavedProgress = {
	cfi: string | null;
	percent: number;
};

type Stored = Progress & { updatedAt: number };

/** The server rejects longer CFIs with 422. */
const MAX_CFI_LENGTH = 2000;

type RemoteProgress = {
	cfi: string | null;
	percent: number | null;
	updated_at: string | null;
};

export interface ProgressStore {
	/** The position to open the book at, or null for a book that hasn't been opened. */
	load(): Promise<SavedProgress | null>;
	/**
	 * Record a new position. Local write is immediate, the server write is debounced.
	 * Only call this once the reader has actually moved: saving the position a book
	 * merely opened at would overwrite a newer one from another device.
	 */
	save(progress: Progress): void;
	/**
	 * Send any pending position now; safe to call while the page is being hidden.
	 * The promise settles when the server has answered (or failed to).
	 */
	flush(): Promise<void>;
}

const DEBOUNCE_MS = 1000;

function parseTimestamp(value: string | null): number {
	if (!value) return 0;
	// SQLite's "YYYY-MM-DD HH:MM:SS" is UTC without saying so.
	const iso = /[zZ]|[+-]\d\d:?\d\d$/.test(value) ? value : value.replace(' ', 'T') + 'Z';
	const ms = Date.parse(iso);
	return Number.isNaN(ms) ? 0 : ms;
}

function storageKey(bookId: number | string): string {
	return `legejo.reader.progress.${bookId}`;
}

/** This device's last known progress (0–1) for a book, without asking the server. */
export function localPercent(bookId: number | string): number | null {
	try {
		const v = JSON.parse(localStorage.getItem(storageKey(bookId)) ?? 'null');
		return typeof v?.percent === 'number' && typeof v?.cfi === 'string' ? v.percent : null;
	} catch {
		return null;
	}
}

export function createProgressStore(bookId: number | string): ProgressStore {
	const key = storageKey(bookId);
	const url = `/api/books/${bookId}/progress`;

	// Set to false once the server answers 404, i.e. it predates the progress API.
	let remote = true;
	let pending: Progress | null = null;
	let timer: ReturnType<typeof setTimeout> | null = null;

	function readLocal(): Stored | null {
		try {
			const raw = localStorage.getItem(key);
			if (!raw) return null;
			const v = JSON.parse(raw);
			if (typeof v?.cfi !== 'string' || !v.cfi) return null;
			return {
				cfi: v.cfi,
				percent: typeof v.percent === 'number' ? v.percent : 0,
				updatedAt: typeof v.updatedAt === 'number' ? v.updatedAt : 0
			};
		} catch {
			return null;
		}
	}

	function writeLocal(progress: Progress) {
		try {
			localStorage.setItem(key, JSON.stringify({ ...progress, updatedAt: Date.now() }));
		} catch {
			// Storage unavailable; the server copy (if any) still works.
		}
	}

	function send(progress: Progress, keepalive: boolean): Promise<void> {
		if (!remote || progress.cfi.length > MAX_CFI_LENGTH) return Promise.resolve();
		const percent = Math.min(1, Math.max(0, progress.percent));
		return fetch(url, {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ cfi: progress.cfi, percent }),
			keepalive
		})
			.then((res) => {
				if (res.status === 404) remote = false;
			})
			.catch(() => {
				// Offline or the server is down: the local copy is the fallback.
			});
	}

	return {
		async load() {
			const local = readLocal();
			let server: RemoteProgress | null = null;
			try {
				const res = await fetch(url);
				if (res.ok) server = await res.json();
				else if (res.status === 404) remote = false;
			} catch {
				// Fall through to the local copy.
			}

			const serverHasPosition = !!server && (server.cfi !== null || server.percent !== null);
			if (server && serverHasPosition) {
				const serverTime = parseTimestamp(server.updated_at);
				if (!local || serverTime >= local.updatedAt) {
					return { cfi: server.cfi, percent: server.percent ?? 0 };
				}
			}
			if (local) {
				// The local copy is newer, or the server has none yet: bring it up to date.
				if (server) send(local, false);
				return { cfi: local.cfi, percent: local.percent };
			}
			return null;
		},

		save(progress) {
			writeLocal(progress);
			pending = progress;
			if (timer) clearTimeout(timer);
			timer = setTimeout(() => {
				timer = null;
				if (pending) send(pending, false);
				pending = null;
			}, DEBOUNCE_MS);
		},

		flush() {
			if (timer) clearTimeout(timer);
			timer = null;
			const last = pending;
			pending = null;
			return last ? send(last, true) : Promise.resolve();
		}
	};
}
