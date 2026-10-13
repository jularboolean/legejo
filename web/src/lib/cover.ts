// A cover that is still being fetched: marked so that its place can shimmer
// and the picture come in once it is there (the styles are in app.css).
//
// Only one that keeps the reader waiting is marked. A cover the browser
// already has is there within a moment of being asked for, though it is not
// "complete" at once when it loads lazily; marking it straight away made
// every cover blink out and in again whenever the list was drawn anew.

/** How long a cover may take before its place shimmers. */
const WAIT_MS = 180;

export function cover(img: HTMLImageElement) {
	img.dataset.cover = '';
	if (img.complete && img.naturalWidth > 0) return;
	let waiting: ReturnType<typeof setTimeout> | null = setTimeout(() => {
		waiting = null;
		if (!img.complete) img.dataset.cover = 'arriving';
	}, WAIT_MS);
	const arrived = () => {
		if (waiting) clearTimeout(waiting);
		waiting = null;
		img.dataset.cover = '';
	};
	img.addEventListener('load', arrived);
	img.addEventListener('error', arrived);
	return {
		destroy() {
			if (waiting) clearTimeout(waiting);
			img.removeEventListener('load', arrived);
			img.removeEventListener('error', arrived);
		}
	};
}
