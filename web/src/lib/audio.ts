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
