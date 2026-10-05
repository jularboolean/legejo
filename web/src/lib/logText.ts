import { t, type MessageKey } from '#lib/i18n';

export type LogEntry = {
	id: number;
	at: string;
	actor_id: number | null;
	actor_name: string | null;
	action: string;
	details: Record<string, unknown>;
};

export type LogPage = { entries: LogEntry[]; next: number | null };

/** The filter groups: the part of the action before the dot. */
export const LOG_GROUPS = ['user', 'account', 'book', 'shelf', 'kobo', 'admin', 'fed'] as const;

const s = (v: unknown) => (v == null ? '' : String(v));

/** One log entry as a sentence (without the actor, shown separately). */
export function logText(e: LogEntry): string {
	const d = e.details ?? {};
	switch (e.action) {
		case 'account.updated': {
			const parts: string[] = [];
			if (d.renamed_from) parts.push(t('log.account.renamed', { from: s(d.renamed_from) }));
			if (d.password_changed) parts.push(t('log.account.password'));
			return parts.length ? parts.join(', ') : t('log.account.updated');
		}
		case 'shelf.edited': {
			const vis = s(d.visibility);
			const key = (`shelf.vis.${vis}` as MessageKey);
			return t('log.shelf.edited', { name: s(d.name), visibility: vis ? t(key) : '' });
		}
		case 'admin.settings':
			return t('log.admin.settings', {
				libris: d.libris_enabled ? t('log.on') : t('log.off'),
				registration: d.registration_enabled ? t('log.on') : t('log.off')
			});
		case 'kobo.synced': {
			// One entry per sync round; a library larger than a round continues.
			const parts: string[] = [];
			if (Number(d.new) > 0) parts.push(t('log.kobo.part.new', { n: s(d.new) }));
			if (Number(d.changed) > 0) parts.push(t('log.kobo.part.changed', { n: s(d.changed) }));
			if (Number(d.collections) > 0) parts.push(t('log.kobo.part.collections', { n: s(d.collections) }));
			if (Number(d.states) > 0) parts.push(t('log.kobo.part.states', { n: s(d.states) }));
			if (parts.length === 0) return t('log.kobo.synced_none');
			const text = t('log.kobo.synced', { what: parts.join(', ') });
			return d.more ? `${text} ${t('log.kobo.more')}` : text;
		}
		case 'book.edited':
			return d.license
				? t('log.book.editedLicense', { title: s(d.title), license: s(d.license) })
				: t('log.book.edited', { title: s(d.title) });
	}
	const key = `log.${e.action}` as MessageKey;
	const text = t(key, Object.fromEntries(Object.entries(d).map(([k, v]) => [k, s(v)])));
	return text === key ? e.action : text;
}

/** Who: the user, or for remote events the remote actor's host. */
export function logActor(e: LogEntry): string {
	if (e.actor_name) return e.actor_name;
	const remote = e.details?.actor;
	if (typeof remote === 'string') {
		try {
			return new URL(remote).host;
		} catch {
			return remote;
		}
	}
	return t('log.system');
}
