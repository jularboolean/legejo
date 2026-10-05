/** Shared helpers for the federation UI. */

/** The host part of a remote IRI, for a short link text; the whole string if it does not parse. */
export function fedHost(iri: string): string {
	try {
		return new URL(iri).host;
	} catch {
		return iri;
	}
}

/** What the server accepts as an ActivityPub handle slug. */
export const SLUG_RE = /^[a-z0-9][a-z0-9-]{1,62}$/;

export function formatSize(bytes: number): string {
	if (bytes >= 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + ' MB';
	return Math.max(1, Math.round(bytes / 1024)) + ' kB';
}
