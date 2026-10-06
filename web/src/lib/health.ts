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
 */
export function canRepair(issue: HealthIssue, book: { language: string | null }): boolean {
	if (issue.fixable || issue.code === 'no_title') return true;
	return issue.code === 'no_language' && !!book.language;
}

export function fixedList(health: Pick<Health, 'fixed'>): string {
	return health.fixed.map(fixedText).join(', ');
}
