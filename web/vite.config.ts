/// <reference types="vitest/config" />
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, sep } from 'node:path';
import { defineConfig, loadEnv, type Plugin } from 'vite';
import pkg from './package.json' with { type: 'json' };

// What pdf.js reads besides its code, for the PDFs that need it: the Adobe
// CMaps (CJK and other text in predefined encodings), the standard fonts a
// PDF names without holding them, the decoders of JPEG 2000 and JBIG2
// pictures (common in scanned books), and the ICC colour profiles. Without
// them such pages come out with text or pictures missing. They are served
// under the version of pdf.js, beside the app's other unchanging files.
const PDFJS_ROOT = dirname(createRequire(import.meta.url).resolve('pdfjs-dist/package.json'));
const PDFJS_VERSION: string = JSON.parse(readFileSync(join(PDFJS_ROOT, 'package.json'), 'utf8')).version;
const PDFJS_PATH = `_app/immutable/pdfjs/${PDFJS_VERSION}/`;
const PDFJS_DIRS = ['cmaps', 'standard_fonts', 'wasm', 'iccs'];
// The engine for scripts in a PDF: the reader runs none.
const PDFJS_LEFT_OUT = /^quickjs/;

function pdfjsResources(): Plugin {
	const files = () =>
		PDFJS_DIRS.flatMap((dir) =>
			readdirSync(join(PDFJS_ROOT, dir))
				.filter((name) => !PDFJS_LEFT_OUT.test(name))
				.map((name) => `${dir}/${name}`)
		);
	return {
		name: 'pdfjs-resources',
		configureServer(server) {
			server.middlewares.use(`/${PDFJS_PATH}`, (req, res, next) => {
				const file = join(PDFJS_ROOT, decodeURIComponent((req.url ?? '').split('?')[0]));
				if (!file.startsWith(PDFJS_ROOT + sep) || !existsSync(file)) return next();
				res.end(readFileSync(file));
			});
		},
		generateBundle() {
			// Once, with the app's own files.
			if (this.environment.name !== 'client') return;
			for (const file of files()) {
				this.emitFile({ type: 'asset', fileName: PDFJS_PATH + file, source: readFileSync(join(PDFJS_ROOT, file)) });
			}
		}
	};
}

// Where the dev server proxies /api and /podcast. Override to run against another instance,
// e.g. LEGEJO_API=http://127.0.0.1:3001 npm run dev -- --port 5174
// (from the environment or an .env file).
const DEFAULT_API = 'http://127.0.0.1:3000';

export default defineConfig(({ mode }) => ({
	define: { __APP_VERSION__: JSON.stringify(pkg.version), __PDFJS_RESOURCES__: JSON.stringify(`/${PDFJS_PATH}`) },
	plugins: [
		pdfjsResources(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},

			// The app is a pure SPA (ssr = false in +layout.ts), so it builds to static
			// files in web/build. In production the Legejo server serves them itself
			// (LEGEJO_WEB_DIR) and answers unknown paths with index.html, which is
			// what `fallback` produces.
			adapter: adapter({ fallback: 'index.html' }),

			// Open tabs survive a deploy: when the app version changes, failed
			// client-side navigations fall back to a full-page load instead of
			// failing dynamic imports of chunks that no longer exist.
			version: { pollInterval: 60_000 },

			// The service worker (src/service-worker) is registered by the app
			// itself, for a logged-in user and only in a built app.
			serviceWorker: { register: false }
		})
	],
	test: { include: ['src/**/*.test.ts'] },
	server: {
		proxy: {
			'/api': loadEnv(mode, '.', 'LEGEJO_').LEGEJO_API || DEFAULT_API,
			// Podcast feeds of audiobooks: fetched by podcast apps, not by the web app.
			'/podcast': loadEnv(mode, '.', 'LEGEJO_').LEGEJO_API || DEFAULT_API,
			// Atom feeds of followed shelves, likewise for feed readers.
			'/feeds': loadEnv(mode, '.', 'LEGEJO_').LEGEJO_API || DEFAULT_API
		}
	}
}));
