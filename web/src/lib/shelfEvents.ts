// The owner's log of a shelf: what happened, in words.

import { t, type MessageKey } from '#lib/i18n';

export type ShelfEventKind =
	| 'followed'
	| 'unfollowed'
	| 'imported'
	| 'link_download'
	| 'fed_followed'
	| 'fed_unfollowed'
	| 'fed_download';

export type ShelfEvent = {
	id: number;
	at: string;
	kind: ShelfEventKind | string;
	actor_id: number | null;
	actor_name: string | null;
	book_id: number | null;
	book_title: string | null;
	detail: string | null;
};

export type ShelfEventPage = { entries: ShelfEvent[]; next: number | null };

const KINDS: ShelfEventKind[] = ['followed', 'unfollowed', 'imported', 'link_download', 'fed_followed', 'fed_unfollowed', 'fed_download'];

/** The event as a sentence; the parts the page sets apart are given separately. */
export function eventText(e: ShelfEvent): string {
	const kind = (KINDS as string[]).includes(e.kind) ? e.kind : 'other';
	return t(`shelfLog.${kind}` as MessageKey, {
		who: e.actor_name ?? t('shelfLog.someone'),
		book: e.book_title ?? t('shelfLog.aBook'),
		link: e.detail ?? ''
	});
}

/** Whether it came from outside the instance. */
export function fromElsewhere(e: ShelfEvent): boolean {
	return e.kind === 'link_download' || e.kind.startsWith('fed_');
}
