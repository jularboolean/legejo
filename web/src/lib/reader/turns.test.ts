import { describe, expect, it } from 'vitest';
import { createTurnGate, isWebKit } from './turns';

/** A one-at-a-time queue like the engine's, with turns that finish on demand. */
function setup() {
	let queue: Promise<void> = Promise.resolve();
	const enqueue = (task: () => Promise<void>) => {
		const run = queue.then(task);
		queue = run.catch(() => {});
		return run;
	};
	const gate = createTurnGate(enqueue);
	const done: string[] = [];
	const release: (() => void)[] = [];
	const turn = (forward: boolean, held = true) =>
		gate(forward, async () => {
			if (held) await new Promise<void>((resolve) => release.push(resolve));
			done.push(forward ? 'next' : 'prev');
		});
	const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
	const finish = async () => {
		await settle();
		release.shift()?.();
		await settle();
	};
	return { turn, done, finish, settle, release };
}

describe('createTurnGate', () => {
	it('runs turns made one after another', async () => {
		const { turn, done } = setup();
		await turn(true, false);
		await turn(false, false);
		await turn(true, false);
		expect(done).toEqual(['next', 'prev', 'next']);
	});

	it('keeps one turn waiting behind the one in progress', async () => {
		const { turn, done, finish, settle } = setup();
		const first = turn(true);
		await settle();
		const all = [first, turn(true), turn(true), turn(true), turn(true)];
		await finish();
		await finish();
		await finish();
		await Promise.all(all);
		await settle();
		expect(done).toEqual(['next', 'next']);
	});

	it('cancels the waiting turn when the reader turns the other way', async () => {
		const { turn, done, finish, settle } = setup();
		const first = turn(true);
		await settle();
		const all = [first, turn(true), turn(false)];
		await finish();
		await finish();
		await Promise.all(all);
		expect(done).toEqual(['next']);
	});

	it('queues a reversal when nothing is waiting', async () => {
		const { turn, done, finish, settle } = setup();
		const first = turn(true);
		await settle();
		const back = turn(false);
		await finish();
		await finish();
		await Promise.all([first, back]);
		expect(done).toEqual(['next', 'prev']);
	});

	it('resolves the promises of cancelled turns', async () => {
		const { turn, finish, settle } = setup();
		const first = turn(true);
		await settle();
		const replaced = turn(true);
		const reversal = turn(false);
		await expect(reversal).resolves.toBeUndefined();
		await finish();
		await expect(first).resolves.toBeUndefined();
		await expect(replaced).resolves.toBeUndefined();
	});

	it('goes on after a turn that fails', async () => {
		let queue: Promise<void> = Promise.resolve();
		const gate = createTurnGate((task) => {
			const run = queue.then(task);
			queue = run.catch(() => {});
			return run;
		});
		await expect(gate(true, () => Promise.reject(new Error('timeout')))).rejects.toThrow('timeout');
		let ran = false;
		await gate(true, async () => {
			ran = true;
		});
		expect(ran).toBe(true);
	});
});

describe('isWebKit', () => {
	it('is Safari on macOS and iOS, and other browsers on iOS', () => {
		expect(
			isWebKit(
				'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15'
			)
		).toBe(true);
		expect(
			isWebKit(
				'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1'
			)
		).toBe(true);
		expect(
			isWebKit(
				'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/130.0.6723.90 Mobile/15E148 Safari/604.1'
			)
		).toBe(true);
	});

	it('is not Chromium or Firefox elsewhere', () => {
		expect(
			isWebKit(
				'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36'
			)
		).toBe(false);
		expect(
			isWebKit(
				'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0'
			)
		).toBe(false);
		expect(
			isWebKit(
				'Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Mobile Safari/537.36'
			)
		).toBe(false);
		expect(isWebKit('Mozilla/5.0 (Macintosh; Intel Mac OS X 14.0; rv:130.0) Gecko/20100101 Firefox/130.0')).toBe(false);
	});
});
