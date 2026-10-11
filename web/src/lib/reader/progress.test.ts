import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { sendUnsent } from './progress';

const KEY = 'legejo.reader.progress.4';

function storage() {
	const map = new Map<string, string>();
	return {
		getItem: (k: string) => map.get(k) ?? null,
		setItem: (k: string, v: string) => void map.set(k, v),
		removeItem: (k: string) => void map.delete(k),
		key: (i: number) => [...map.keys()][i] ?? null,
		get length() {
			return map.size;
		}
	};
}

function answer(body: unknown, init: { status?: number; headers?: Record<string, string> } = {}) {
	return new Response(JSON.stringify(body), { status: init.status ?? 200, headers: init.headers });
}

describe('sendUnsent', () => {
	beforeEach(() => vi.stubGlobal('localStorage', storage()));
	afterEach(() => vi.unstubAllGlobals());

	const unsent = (updatedAt: number) => localStorage.setItem(KEY, JSON.stringify({ cfi: 'epubcfi(/6/4!/4/2)', percent: 0.4, updatedAt, unsent: true }));
	const stored = () => JSON.parse(localStorage.getItem(KEY)!);

	it('sends a position read without the network, once the server answers', async () => {
		unsent(Date.parse('2026-10-11T10:00:00Z'));
		const fetch = vi.fn(async (_url: string, init?: RequestInit) => (init?.method === 'PUT' ? answer({}) : answer({ cfi: 'old', percent: 0.1, updated_at: '2026-10-10T10:00:00Z' })));
		vi.stubGlobal('fetch', fetch);
		await sendUnsent();
		expect(fetch).toHaveBeenCalledTimes(2);
		expect(fetch.mock.calls[1][0]).toBe('/api/books/4/progress');
		expect(JSON.parse(fetch.mock.calls[1][1]!.body as string)).toEqual({ cfi: 'epubcfi(/6/4!/4/2)', percent: 0.4 });
		expect(stored().unsent).toBeUndefined();
	});

	it('gives way to a later position from another device', async () => {
		unsent(Date.parse('2026-10-11T10:00:00Z'));
		const fetch = vi.fn(async () => answer({ cfi: 'newer', percent: 0.9, updated_at: '2026-10-11T12:00:00Z' }));
		vi.stubGlobal('fetch', fetch);
		await sendUnsent();
		expect(fetch).toHaveBeenCalledTimes(1);
		expect(stored().unsent).toBeUndefined();
	});

	it('waits while the answer comes from the device itself, or not at all', async () => {
		unsent(Date.parse('2026-10-11T10:00:00Z'));
		const cached = vi.fn(async () => answer({ cfi: 'old', percent: 0.1, updated_at: '2026-10-10T10:00:00Z' }, { headers: { 'x-legejo-cached': '1' } }));
		vi.stubGlobal('fetch', cached);
		await sendUnsent();
		expect(cached).toHaveBeenCalledTimes(1);
		expect(stored().unsent).toBe(true);

		vi.stubGlobal('fetch', vi.fn(async () => Promise.reject(new TypeError('offline'))));
		await sendUnsent();
		expect(stored().unsent).toBe(true);
	});

	it('leaves what the server already has alone', async () => {
		localStorage.setItem(KEY, JSON.stringify({ cfi: 'epubcfi(/6/4!/4/2)', percent: 0.4, updatedAt: 1 }));
		const fetch = vi.fn();
		vi.stubGlobal('fetch', fetch);
		await sendUnsent();
		expect(fetch).not.toHaveBeenCalled();
	});
});
