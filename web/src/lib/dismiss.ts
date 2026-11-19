// Closing a modal <dialog> by a click on its backdrop. A click's target is
// the dialog itself both on the backdrop and on the dialog's own padding,
// and a text selection dragged out of the dialog ends on the backdrop; so
// the pointer's position is tested instead, and both the press and the
// release must be outside.

function outside(e: PointerEvent, dialog: HTMLDialogElement): boolean {
	const r = dialog.getBoundingClientRect();
	return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
}

/** Svelte action: `use:dismissOnBackdrop={onclose}`. */
export function dismissOnBackdrop(dialog: HTMLDialogElement, onclose: () => void) {
	let close = onclose;
	let pressedOutside = false;
	const down = (e: PointerEvent) => {
		pressedOutside = outside(e, dialog);
	};
	const up = (e: PointerEvent) => {
		if (pressedOutside && outside(e, dialog)) close();
		pressedOutside = false;
	};
	dialog.addEventListener('pointerdown', down);
	dialog.addEventListener('pointerup', up);
	return {
		update(next: () => void) {
			close = next;
		},
		destroy() {
			dialog.removeEventListener('pointerdown', down);
			dialog.removeEventListener('pointerup', up);
		}
	};
}
