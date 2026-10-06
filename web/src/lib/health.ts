import { t, type MessageKey } from './i18n';
import type { Health, HealthIssue } from './types';

/** What the health check says, in words. */
export function issueText(issue: HealthIssue): string {
	return t(`health.issue.${issue.code}` as MessageKey, { count: issue.count ?? 0 });
}

/** The short name of a repair, for "Repaired: …". */
export function fixedText(code: string): string {
	return t(`health.fixed.${code}` as MessageKey);
}

/**
 * Whether asking for a repair would get rid of the issue. A missing title or
 * language is written from the catalog when the catalog has one.
 * A missing cover is put in from the catalog when the catalog has one.
 */
export function canRepair(issue: HealthIssue, book: { language: string | null; has_cover: boolean }): boolean {
	if (issue.fixable || issue.code === 'no_title') return true;
	if (issue.code === 'no_cover') return book.has_cover;
	return issue.code === 'no_language' && !!book.language;
}

/**
 * Whether there is something to do about the issue: a repair, or something
 * the owner can add or decide. The rest is detail about a file that reads as
 * it is. The server counts the same way for the library's filter.
 */
export function needsAttention(issue: HealthIssue): boolean {
	return issue.fixable || ['encrypted', 'no_title', 'no_language', 'no_cover'].includes(issue.code);
}

export function fixedList(health: Pick<Health, 'fixed'>): string {
	return health.fixed.map(fixedText).join(', ');
}
