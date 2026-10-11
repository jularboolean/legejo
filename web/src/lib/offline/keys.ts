// Names shared by the service worker and the page: which caches there are,
// and which requests belong to a book saved on the device.

/** The books saved on the device: file, cover and what the reader asks for. */
export const BOOKS_CACHE = 'legejo-books-v1';
/** Who is logged in and what the menus need, as last seen. */
export const DATA_CACHE = 'legejo-data-v1';
/** Set on an answer that came from a cache because the network did not answer. */
export const CACHED_HEADER = 'x-legejo-cached';

/** What every page asks for before it shows anything. */
export const SESSION_PATHS = ['/api/auth/me', '/api/config', '/api/shelves', '/api/fed/status'];

export type BookPart = 'detail' | 'file' | 'cover' | 'progress' | 'annotations';

const PARTS: Record<string, BookPart> = { '': 'detail', '/file': 'file', '/cover': 'cover', '/progress': 'progress', '/annotations': 'annotations' };

/** The saved-book request a path is, with the address it is kept under. */
export function bookKey(path: string): { id: string; kind: BookPart; url: string } | null {
	const match = /^\/api\/books\/(\d+)(|\/file|\/cover|\/progress|\/annotations)$/.exec(path);
	if (!match) return null;
	return { id: match[1], kind: PARTS[match[2]], url: path };
}

/** The addresses kept for a saved book. The file is the last to be written: it is what makes the book saved. */
export function bookUrls(id: number | string): Record<BookPart, string> {
	const base = `/api/books/${id}`;
	return { detail: base, cover: `${base}/cover`, progress: `${base}/progress`, annotations: `${base}/annotations`, file: `${base}/file` };
}
