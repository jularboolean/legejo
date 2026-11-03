// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
declare global {
	const __APP_VERSION__: string;
	/** Where pdf.js's CMaps, fonts, decoders and colour profiles are, with a trailing slash. */
	const __PDFJS_RESOURCES__: string;
	namespace App {
		// interface Error {}
		// interface Locals {}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

export {};
