import { primaryLanguage } from '#lib/library';
import { fold } from '#lib/library/authors';
import type { Audiobook } from '#lib/types';

/** "6 h 55 min", "29 min" or "45 s", for a length in seconds. */
export function formatLength(seconds: number): string {
	if (seconds <= 0) return '–';
	const h = Math.floor(seconds / 3600);
	const m = Math.round((seconds % 3600) / 60);
	if (h > 0) return m > 0 ? `${h} h ${m} min` : `${h} h`;
	if (m > 0) return `${m} min`;
	return `${seconds} s`;
}

export function formatBytes(bytes: number): string {
	if (bytes >= 1024 * 1024 * 1024) return (bytes / 1024 / 1024 / 1024).toFixed(1) + ' GB';
	if (bytes >= 1024 * 1024) return Math.round(bytes / 1024 / 1024) + ' MB';
	return Math.max(1, Math.round(bytes / 1024)) + ' kB';
}

const AUDIO = /\.(mp3|m4a|m4b)$/i;

/** The audio files of a selection, in listening order: by name, numbers by value. */
export function audioFiles(files: Iterable<File>): File[] {
	const order = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });
	const path = (f: File) => f.webkitRelativePath || f.name;
	return [...files].filter((f) => AUDIO.test(f.name)).sort((a, b) => order.compare(path(a), path(b)));
}

/** Send one file to an audiobook, reporting how much of it has left the browser. */
export function sendPart(
	id: number,
	file: File,
	onProgress: (sent: number) => void
): Promise<{ ok: boolean; error?: string }> {
	return new Promise((resolve) => {
		const form = new FormData();
		form.append('files', file);
		const xhr = new XMLHttpRequest();
		xhr.open('POST', `/api/audiobooks/${id}/files`);
		xhr.upload.onprogress = (e) => {
			if (e.lengthComputable) onProgress(Math.min(file.size, Math.round((e.loaded / e.total) * file.size)));
		};
		xhr.onload = () => {
			if (xhr.status === 413) return resolve({ ok: false, error: 'too-large' });
			try {
				const body = JSON.parse(xhr.responseText);
				if (xhr.status >= 200 && xhr.status < 300 && !(body.errors?.length > 0)) return resolve({ ok: true });
				resolve({ ok: false, error: body.errors?.[0] });
			} catch {
				resolve({ ok: false });
			}
		};
		xhr.onerror = () => resolve({ ok: false, error: 'network' });
		xhr.send(form);
	});
}

/** What the list of audiobooks is narrowed by; null and '' leave a field alone. */
export type AudioFilters = {
	text: string;
	tag: string | null;
	language: string | null;
	category: string | null;
	/** 'mine' or 'shared' (with the user, by someone else). */
	owner: string | null;
};

export function emptyAudioFilters(): AudioFilters {
	return { text: '', tag: null, language: null, category: null, owner: null };
}

/** The text is looked for in the title, the author, the reader and the description. */
export function filterAudiobooks(books: Audiobook[], f: AudioFilters): Audiobook[] {
	const words = fold(f.text).split(/\s+/).filter(Boolean);
	return books.filter((book) => {
		if (f.owner && (f.owner === 'mine') !== book.mine) return false;
		if (f.tag && !book.tags.some((tag) => fold(tag) === f.tag)) return false;
		if (f.category && fold(book.category ?? '') !== f.category) return false;
		if (f.language && primaryLanguage(book.language) !== f.language) return false;
		if (words.length === 0) return true;
		const hay = fold([book.title, book.author, book.narrator, book.description].filter(Boolean).join(' '));
		return words.every((word) => hay.includes(word));
	});
}

/** The values in use, most common first, keyed without case or accents. */
export function counted(values: string[]): { key: string; label: string; count: number }[] {
	const found = new Map<string, { key: string; label: string; count: number }>();
	for (const value of values) {
		const key = fold(value);
		const entry = found.get(key);
		if (entry) entry.count += 1;
		else found.set(key, { key, label: value, count: 1 });
	}
	return [...found.values()].sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));
}

const FILTERS_KEY = 'legejo.audiobooks.filters';

/** Filters last for the session, so they survive opening an audiobook and coming back. */
export function loadAudioFilters(): AudioFilters {
	const filters = emptyAudioFilters();
	try {
		const saved = JSON.parse(sessionStorage.getItem(FILTERS_KEY) ?? 'null');
		if (typeof saved?.text === 'string') filters.text = saved.text;
		for (const field of ['tag', 'language', 'category', 'owner'] as const) {
			if (typeof saved?.[field] === 'string') filters[field] = saved[field];
		}
	} catch {
		// No storage, or something else's data: no filters.
	}
	return filters;
}

export function saveAudioFilters(filters: AudioFilters) {
	try {
		sessionStorage.setItem(FILTERS_KEY, JSON.stringify(filters));
	} catch {
		// Private windows and full disks: the filters just don't stick.
	}
}
