/** The page to go to after login: one of this site's own, else the start page. */
export function nextPage(next: string | null): string {
	if (!next || !next.startsWith('/') || next.startsWith('//') || next.startsWith('/api/')) return '/';
	return next;
}
