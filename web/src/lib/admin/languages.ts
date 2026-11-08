/** Admin → Languages: the language packs of the instance (server/src/langadmin.rs). */

export type LanguagePack = {
	code: string;
	name: string;
	version: string;
	source: string;
	license: string;
	attribution: string;
	built_in: boolean;
	/** Imported for a language the server has a built-in pack for. */
	replaces_built_in: boolean;
	lemmas: number;
	forms: number;
	/** Books on the instance in the language, of every user. */
	books: number;
};

export type LanguagePacks = { packs: LanguagePack[]; catalogue: string };

/** The largest zip the server takes. */
export const MAX_PACK_ZIP_BYTES = 64 * 1024 * 1024;

/** A failed request: the status and the server's message, when it gave one. */
export class PackError extends Error {
	constructor(
		readonly status: number,
		message: string
	) {
		super(message);
	}
}

async function failure(res: Response): Promise<PackError> {
	const body = await res.json().catch(() => null);
	return new PackError(res.status, typeof body?.error === 'string' ? body.error : '');
}

export async function listPacks(fetcher: typeof fetch = fetch): Promise<LanguagePacks> {
	const res = await fetcher('/api/admin/languages');
	if (!res.ok) throw await failure(res);
	return res.json();
}

export async function importPack(file: File): Promise<LanguagePack> {
	const form = new FormData();
	form.append('file', file);
	const res = await fetch('/api/admin/languages', { method: 'POST', body: form });
	if (!res.ok) throw await failure(res);
	return res.json();
}

export async function removePack(code: string): Promise<void> {
	const res = await fetch(`/api/admin/languages/${encodeURIComponent(code)}`, { method: 'DELETE' });
	if (!res.ok) throw await failure(res);
}
