import { describe, expect, it } from 'vitest';
import { setLocale } from '#lib/i18n';
import { en } from '#lib/i18n/en';
import { LOG_GROUPS, logActor, logText, type LogEntry } from '#lib/logText';

const entry = (action: string, details: Record<string, unknown> = {}): LogEntry => ({
	id: 1,
	at: '2026-01-01T00:00:00.000Z',
	actor_id: 1,
	actor_name: 'ada',
	action,
	details
});

/** Every .rs file of the server, as text. */
const serverSources = Object.values(
	import.meta.glob<string>('../../../server/src/**/*.rs', { query: '?raw', import: 'default', eager: true })
);

/**
 * The actions the server writes to the log: the literal handed to
 * `audit::log`, and any other string of the form "group.name" for a known
 * group (which covers actions chosen in a variable first).
 */
function loggedActions(): string[] {
	const groups = LOG_GROUPS.join('|');
	const literal = new RegExp(`"((?:${groups})\\.[a-z_]+(?:\\.[a-z_]+)*)"`, 'g');
	const call = /audit::log\(\s*&?\w+,\s*[^,]+?,\s*"([^"]+)"/g;
	// Strings of the same shape that are not actions.
	const notActions = new Set(['book.epub']);
	const found = new Set<string>();
	for (const source of serverSources) {
		for (const m of source.matchAll(call)) found.add(m[1]);
		for (const m of source.matchAll(literal)) found.add(m[1]);
	}
	return [...found].filter((a) => !notActions.has(a)).sort();
}

describe('the system log', () => {
	setLocale('en');

	it('reads the server\'s source', () => {
		expect(serverSources.length).toBeGreaterThan(20);
	});

	it('has a text for every action the server logs', () => {
		const actions = loggedActions();
		// The search itself must keep working: these are logged for certain.
		expect(actions).toEqual(expect.arrayContaining(['book.uploaded', 'audiobook.added', 'book.sent_to_kindle']));
		const special = ['account.updated', 'shelf.edited', 'admin.settings', 'kobo.synced', 'book.edited'];
		const missing = actions.filter((action) => !special.includes(action) && !(`log.${action}` in en));
		expect(missing, 'actions without a text in the web app (add log.<action> to the translations)').toEqual([]);
	});

	it('has a filter group for every action the server logs', () => {
		const groups = new Set<string>(LOG_GROUPS);
		const stray = loggedActions().filter((action) => !groups.has(action.split('.')[0]));
		expect(stray, 'actions outside the filter groups (add the group to LOG_GROUPS)').toEqual([]);
	});

	it('writes every logged action as a sentence, not as its code', () => {
		for (const action of loggedActions()) {
			const text = logText(entry(action, { title: 'Dracula', name: 'Gothic', visibility: 'private' }));
			expect(text, action).not.toBe(action);
			expect(text.length, action).toBeGreaterThan(0);
		}
	});

	it('shows an action it has no text for as its code', () => {
		expect(logText(entry('future.thing', { title: 'x' }))).toBe('future.thing');
		expect(logText(entry('future.thing'))).toBe('future.thing');
	});

	it('fills in the details', () => {
		expect(logText(entry('audiobook.deleted', { title: 'Kalevala' }))).toBe('deleted the audiobook “Kalevala”');
		expect(logText(entry('book.sent_to_kindle', { title: 'Dracula' }))).toBe('sent “Dracula” to their Kindle');
		expect(logText(entry('kobo.synced', {}))).toBe(en['log.kobo.synced_none']);
	});

	it('names who did it', () => {
		expect(logActor(entry('book.uploaded'))).toBe('ada');
		const remote = { ...entry('fed.followed', { actor: 'https://example.org/users/x' }), actor_name: null };
		expect(logActor(remote)).toBe('example.org');
	});
});
