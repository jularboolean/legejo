// Pointer, touch, wheel and keyboard input for the reading surface.
//
// Book content is rendered in iframes, and events inside an iframe never reach
// the parent document. The same handlers are therefore attached to every
// chapter document (via the engine's `onContent`) and to the reader's own
// elements; coordinates are normalised to the top window.
//
// WebKit (Safari, and every browser on iOS) doesn't run listeners in a
// sandboxed iframe without `allow-scripts`, which book content never gets.
// There the reader makes the iframes transparent to the pointer instead, so
// input lands on its own element underneath, and looks up what was hit in the
// chapter itself (`linkAt`).

export type TapZone = 'start' | 'center' | 'end';

export type GestureHandlers = {
	/** The reading area in top-window coordinates; tap zones are thirds of it. */
	area: () => DOMRect;
	onTap: (zone: TapZone, point: { x: number; y: number }) => void;
	/** A horizontal swipe; `forward` is right-to-left. */
	onSwipe: (forward: boolean) => void;
	/** Return true when the key was handled, to suppress its default action. */
	onKey: (event: KeyboardEvent) => boolean;
	onWheel?: (deltaY: number) => void;
	/** A link in the book was activated. */
	onLink?: () => void;
	/** The mouse moved (not touch); coordinates in the top window. */
	onPointer?: (point: { x: number; y: number }) => void;
	/** The reader touched the content in a way that may scroll it. */
	onActivity?: () => void;
	/** The selection in a chapter document may have changed (it may be empty). */
	onSelect?: () => void;
	/** A finger or button held still on a point of the top window. */
	onLongPress?: (point: { x: number; y: number }) => void;
	/** The mouse was dragged from one point of the top window to another. */
	onDragSelect?: (from: { x: number; y: number }, to: { x: number; y: number }) => void;
};

const LONG_PRESS_MS = 550;
const LONG_PRESS_SLOP_PX = 10;

const SWIPE_MIN_PX = 45;
const SWIPE_MAX_MS = 700;
const INTERACTIVE = 'a[href], button, input, textarea, select, label, summary, audio, video, [role="button"]';

/** Offset of a document's viewport inside the top window (zero for the top document). */
function frameOffset(doc: Document): { x: number; y: number } {
	const frame = doc.defaultView?.frameElement;
	if (!frame) return { x: 0, y: 0 };
	const rect = frame.getBoundingClientRect();
	return { x: rect.left, y: rect.top };
}

/**
 * Attach reader input handling to a chapter document or to an element of the
 * reader itself. Returns a function that detaches it again.
 */
export function attachGestures(target: Document | HTMLElement, handlers: GestureHandlers): () => void {
	// Not `instanceof`: a chapter document belongs to its iframe's realm.
	const isDocument = target.nodeType === Node.DOCUMENT_NODE;
	const doc = isDocument ? (target as Document) : (target as HTMLElement).ownerDocument;
	const controller = new AbortController();
	const { signal } = controller;

	let touch: { x: number; y: number; time: number } | null = null;
	let lastSwipe = 0;
	let press: { x: number; y: number; timer: number } | null = null;

	// A long press: held still for a moment, on a touch screen or with a
	// mouse button. The tap or swipe that would follow is then not one.
	const pressEnd = () => {
		if (press) window.clearTimeout(press.timer);
		press = null;
	};
	const pressStart = (x: number, y: number) => {
		pressEnd();
		press = {
			x,
			y,
			timer: window.setTimeout(() => {
				press = null;
				touch = null;
				lastSwipe = Date.now();
				const offset = frameOffset(doc);
				handlers.onLongPress?.({ x: x + offset.x, y: y + offset.y });
			}, LONG_PRESS_MS)
		};
	};
	const pressMove = (x: number, y: number) => {
		if (press && (Math.abs(x - press.x) > LONG_PRESS_SLOP_PX || Math.abs(y - press.y) > LONG_PRESS_SLOP_PX)) pressEnd();
	};

	const on = <K extends keyof DocumentEventMap>(
		type: K,
		listener: (event: DocumentEventMap[K]) => void,
		passive = true
	) => target.addEventListener(type, listener as EventListener, { signal, passive });

	on('click', (event) => {
		if (event.button !== 0) return;
		if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
		if (Date.now() - lastSwipe < 400) return;
		const el = event.target as Element | null;
		const link = isDocument ? (el?.closest?.('a[href]') as HTMLAnchorElement | null) : null;
		if (link) {
			// Links inside the book are followed by epub.js, whose handler has
			// already run and cancelled the click; it still counts as moving.
			// Links out of the book can't open from the sandbox.
			if (!openExternal(link) && typeof link.onclick === 'function') handlers.onLink?.();
			return;
		}
		if (event.defaultPrevented || el?.closest?.(INTERACTIVE)) return;
		const selection = doc.getSelection();
		if (selection && !selection.isCollapsed) return;

		const offset = frameOffset(doc);
		const x = event.clientX + offset.x;
		const y = event.clientY + offset.y;
		const area = handlers.area();
		const ratio = area.width > 0 ? (x - area.left) / area.width : 0.5;
		handlers.onTap(ratio < 1 / 3 ? 'start' : ratio > 2 / 3 ? 'end' : 'center', { x, y });
	});

	on('touchstart', (event) => {
		const t = event.touches.length === 1 ? event.touches[0] : null;
		touch = t ? { x: t.screenX, y: t.screenY, time: Date.now() } : null;
		if (t && handlers.onLongPress) pressStart(t.clientX, t.clientY);
		else pressEnd();
	});
	on('touchmove', (event) => {
		handlers.onActivity?.();
		const t = event.touches[0];
		if (t) pressMove(t.clientX, t.clientY);
	});
	on('touchend', (event) => {
		pressEnd();
		const start = touch;
		touch = null;
		const t = event.changedTouches[0];
		if (!start || !t) return;
		const dx = t.screenX - start.x;
		const dy = t.screenY - start.y;
		if (Date.now() - start.time > SWIPE_MAX_MS) return;
		if (Math.abs(dx) < SWIPE_MIN_PX || Math.abs(dx) < Math.abs(dy) * 1.5) return;
		const selection = doc.getSelection();
		if (selection && !selection.isCollapsed) return;
		lastSwipe = Date.now();
		handlers.onSwipe(dx < 0);
	});
	on('touchcancel', () => {
		pressEnd();
		touch = null;
	});
	// A mouse drag: from where the button went down to where it came up. The
	// click that follows it is not a tap.
	let drag: { x: number; y: number } | null = null;
	on('mousedown', (event) => {
		if (event.button !== 0) return;
		if (handlers.onLongPress) pressStart(event.clientX, event.clientY);
		drag = handlers.onDragSelect ? { x: event.clientX, y: event.clientY } : null;
	});
	on('mousemove', (event) => pressMove(event.clientX, event.clientY));
	on('mouseup', (event) => {
		pressEnd();
		const from = drag;
		drag = null;
		if (!from || event.button !== 0) return;
		if (Math.abs(event.clientX - from.x) < LONG_PRESS_SLOP_PX && Math.abs(event.clientY - from.y) < LONG_PRESS_SLOP_PX) return;
		lastSwipe = Date.now();
		const offset = frameOffset(doc);
		handlers.onDragSelect?.(
			{ x: from.x + offset.x, y: from.y + offset.y },
			{ x: event.clientX + offset.x, y: event.clientY + offset.y }
		);
	});

	// A selection is made with the pointer or the keyboard; the reader asks
	// what it is once the event has settled.
	if (isDocument && handlers.onSelect) {
		const settle = () => window.setTimeout(() => handlers.onSelect?.(), 0);
		on('mouseup', settle);
		on('touchend', settle);
		on('keyup', (event) => {
			if (event.shiftKey) settle();
		});
		on('selectionchange', settle);
	}

	// Mouse movement over the reader's own elements reaches its window anyway;
	// only chapter documents need to pass it on.
	if (isDocument) {
		on('pointermove', (event) => {
			if (event.pointerType !== 'mouse' || !handlers.onPointer) return;
			const offset = frameOffset(doc);
			handlers.onPointer({ x: event.clientX + offset.x, y: event.clientY + offset.y });
		});
	}

	on('wheel', (event) => {
		handlers.onActivity?.();
		if (!event.ctrlKey) handlers.onWheel?.(event.deltaY);
	});

	on(
		'keydown',
		(event) => {
			if (event.defaultPrevented) return;
			const el = event.target as Element | null;
			if (el?.closest?.('input, textarea, select, [contenteditable="true"]')) return;
			if (handlers.onKey(event)) event.preventDefault();
		},
		false
	);

	return () => controller.abort();
}

/**
 * Whether listeners on a chapter document are actually invoked. WebKit refuses
 * to run them in a sandboxed iframe without `allow-scripts` (which the reader
 * never grants to book content); see the top of this file.
 */
export function frameEventsWork(doc: Document): boolean {
	let fired = false;
	const probe = () => (fired = true);
	try {
		doc.addEventListener('legejo-probe', probe);
		doc.dispatchEvent(new Event('legejo-probe'));
		doc.removeEventListener('legejo-probe', probe);
	} catch {
		return false;
	}
	return fired;
}

/**
 * Open a link that leads out of the book in a new tab. The sandbox around book
 * content blocks that, so it is done from here. Only web addresses qualify.
 */
function openExternal(link: HTMLAnchorElement): boolean {
	let url: URL;
	try {
		url = new URL(link.getAttribute('href') ?? '', link.baseURI);
	} catch {
		return false;
	}
	// epub.js gives links within the book an onclick handler and leaves the rest.
	if (typeof link.onclick === 'function') return false;
	if (url.protocol !== 'http:' && url.protocol !== 'https:') return false;
	window.open(url.href, '_blank', 'noopener,noreferrer');
	return true;
}

/** The book link under a point of the top window, if any. */
export function linkAt(root: HTMLElement, x: number, y: number): HTMLAnchorElement | null {
	for (const frame of root.querySelectorAll('iframe')) {
		const rect = frame.getBoundingClientRect();
		if (x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) continue;
		const hit = frame.contentDocument?.elementFromPoint(x - rect.left, y - rect.top);
		const link = hit?.closest?.('a[href]') as HTMLAnchorElement | null;
		if (link) return link;
	}
	return null;
}

/**
 * Follow the book link under a point of the top window, if there is one. For
 * when clicks never reach the chapter documents. epub.js stores its navigation
 * handler on each internal link's `onclick`.
 */
export function followLinkAt(root: HTMLElement, x: number, y: number): boolean {
	const link = linkAt(root, x, y);
	if (!link) return false;
	if (openExternal(link)) return true;
	if (typeof link.onclick !== 'function') return false;
	link.onclick(new PointerEvent('click'));
	return true;
}
