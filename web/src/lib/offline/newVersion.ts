// Moving on to a new version of the app. A page load alone does not do it
// where a service worker is in charge: the worker answers with the version
// it holds the files of (service-worker/index.ts), and the new worker only
// takes over once it has fetched the new files. So the new worker is asked
// for first, and the page is loaded once it is in charge.

import { DATA_CACHE, PICTURES_CACHE, REFILL_SHELL, SHELL_PREFIX } from './keys';

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

/** Have the worker in charge take the app's files anew, for as long as it may take within reason. */
async function refilled(): Promise<void> {
	const worker = navigator.serviceWorker.controller;
	if (!worker) return;
	await new Promise<void>((resolve) => {
		const timer = setTimeout(resolve, PATIENCE_MS);
		const channel = new MessageChannel();
		channel.port1.onmessage = () => {
			clearTimeout(timer);
			resolve();
		};
		worker.postMessage(REFILL_SHELL, [channel.port2]);
	});
}

/**
 * Take the app anew from the server, when the device holds on to something
 * wrong: all that can be fetched again goes, and the page loads without it.
 * The saved books stay, and so does what is in local storage (places not yet
 * sent). Which is why the worker is kept: in WebKit the saved books do not
 * outlive its unregistering. Without a server to load from nothing is touched,
 * as the app would not open again. Returns false then.
 */
export async function startOver(): Promise<boolean> {
	const page = await fetch('/', { cache: 'no-store' }).catch(() => null);
	if (!page?.ok) return false;
	await newWorker().catch(() => {});
	for (const name of await caches.keys()) {
		if (name.startsWith(SHELL_PREFIX) || name === DATA_CACHE || name === PICTURES_CACHE) await caches.delete(name);
	}
	await refilled();
	location.reload();
	return true;
}
