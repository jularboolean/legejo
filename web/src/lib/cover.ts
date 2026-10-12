// A cover that is still being fetched: marked so that its place can shimmer
// and the picture come in once it is there (the styles are in app.css). One
// the browser already has is shown at once.

export function cover(img: HTMLImageElement) {
	img.dataset.cover = '';
	if (img.complete && img.naturalWidth > 0) return;
	img.dataset.cover = 'arriving';
	const arrived = () => (img.dataset.cover = '');
	img.addEventListener('load', arrived);
	img.addEventListener('error', arrived);
	return {
		destroy() {
			img.removeEventListener('load', arrived);
			img.removeEventListener('error', arrived);
		}
	};
}
