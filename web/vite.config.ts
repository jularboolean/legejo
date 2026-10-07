import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, loadEnv } from 'vite';
import pkg from './package.json' with { type: 'json' };

// Where the dev server proxies /api and /podcast. Override to run against another instance,
// e.g. LEGEJO_API=http://127.0.0.1:3001 npm run dev -- --port 5174
// (from the environment or an .env file).
const DEFAULT_API = 'http://127.0.0.1:3000';

export default defineConfig(({ mode }) => ({
	define: { __APP_VERSION__: JSON.stringify(pkg.version) },
	plugins: [
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
			version: { pollInterval: 60_000 }
		})
	],
	server: {
		proxy: {
			'/api': loadEnv(mode, '.', 'LEGEJO_').LEGEJO_API || DEFAULT_API,
			// Podcast feeds of audiobooks: fetched by podcast apps, not by the web app.
			'/podcast': loadEnv(mode, '.', 'LEGEJO_').LEGEJO_API || DEFAULT_API
		}
	}
}));
