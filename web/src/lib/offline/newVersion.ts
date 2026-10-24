// Moving on to a new version of the app. A page load alone does not do it
// where a service worker is in charge: the worker answers with the version
// it holds the files of (service-worker/index.ts), and the new worker only
// takes over once it has fetched the new files. So the new worker is asked
// for first, and the page is loaded once it is in charge.

/** How long the new worker is waited for before the page is loaded all the same. */
const PATIENCE_MS = 15_000;

/** Wait until a worker of the new version is in charge of the page, if there is one to wait for. */
async function newWorker(): Promise<void> {
	if (typeof navigator === 'undefined' || !navigator.serviceWorker?.controller) return;
	const registration = await navigator.serviceWorker.getRegistration();
	if (!registration) return;
	await registration.update();
	const incoming = registration.installing ?? registration.waiting;
	if (!incoming) return;
	// It takes over on its own once installed (skipWaiting, clients.claim).
	await new Promise<void>((resolve) => {
		const timer = setTimeout(resolve, PATIENCE_MS);
		const done = () => {
			clearTimeout(timer);
			resolve();
		};
		navigator.serviceWorker.addEventListener('controllerchange', done, { once: true });
		incoming.addEventListener('statechange', () => {
			if (incoming.state === 'redundant') done();
		});
	});
}

/** Load the page, or `href`, in the new version of the app. */
export async function toNewVersion(href?: string): Promise<void> {
	try {
		await newWorker();
	} catch {
		// No worker to ask: a plain page load is all there is.
	}
	if (href) location.href = href;
	else location.reload();
}
