// Is there a newer Legejo than this one? Asked of GitHub from the admin's
// browser, and only when the admin presses the button: the server itself
// never asks anyone anything.

export const RELEASES_URL = 'https://api.github.com/repos/jularboolean/legejo/releases/latest';
export const CHANGELOG_URL = 'https://github.com/jularboolean/legejo/blob/main/CHANGELOG.md';

export type Release = { version: string; url: string };

/** "v1.14.1" or "1.14.1" → [1, 14, 1]; null for anything else. */
export function parseVersion(text: string): number[] | null {
	const m = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(text.trim());
	return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

/** Whether `candidate` is a later version than `current`; false when either is not a version. */
export function isNewer(candidate: string, current: string): boolean {
	const a = parseVersion(candidate);
	const b = parseVersion(current);
	if (!a || !b) return false;
	for (let i = 0; i < 3; i++) {
		if (a[i] !== b[i]) return a[i] > b[i];
	}
	return false;
}

/** The latest release as GitHub describes it, or null when the answer is not one. */
export function releaseFrom(data: unknown): Release | null {
	const d = data as { tag_name?: unknown; html_url?: unknown } | null;
	if (!d || typeof d.tag_name !== 'string' || !parseVersion(d.tag_name)) return null;
	return {
		version: d.tag_name.replace(/^v/, ''),
		url: typeof d.html_url === 'string' ? d.html_url : CHANGELOG_URL
	};
}

/** Asks GitHub for the latest release. Throws when GitHub cannot be reached or answers oddly. */
export async function latestRelease(fetchFn: typeof fetch = fetch): Promise<Release> {
	const res = await fetchFn(RELEASES_URL, { headers: { accept: 'application/vnd.github+json' } });
	if (!res.ok) throw new Error(`GitHub answered ${res.status}`);
	const release = releaseFrom(await res.json());
	if (!release) throw new Error('not a release');
	return release;
}
