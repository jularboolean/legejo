// The browser tab's title, from the route and the page's data: "Röda rummet
// – Legejo", "My library – Legejo". Set in one place (the root layout),
// so there is never more than one <title>.

import { t, type MessageKey } from '#lib/i18n';

type Data = Record<string, any>;

const FIXED: Record<string, MessageKey> = {
	'/(app)': 'home.title',
	'/(app)/authors': 'authors.heading',
	'/(app)/reading': 'reading.heading',
	'/(app)/search': 'search.heading',
	'/(app)/public': 'public.heading',
	'/(app)/fediverse': 'fed.heading',
	'/(app)/admin': 'admin.heading',
	'/(app)/admin/log': 'log.heading',
	'/(app)/account': 'account.heading',
	'/(app)/login': 'login.submit',
	'/(app)/register': 'register.heading',
	'/(app)/forgot': 'forgot.heading',
	'/(app)/reset': 'reset.heading',
	'/(app)/verify': 'register.heading',
	'/(app)/invite': 'register.heading'
};

function part(route: string | null, d: Data): string | null {
	if (!route) return null;
	switch (route) {
		case '/(app)/books/[id]':
		case '/books/[id]/read':
			return d.book?.title ?? null;
		case '/(app)/books/[id]/edit':
			return d.book ? `${d.book.title} · ${t('edit.heading')}` : t('edit.heading');
		case '/(app)/shelves/[id]':
			return d.shelf?.name ?? null;
		case '/(app)/shelves/[id]/edit':
			return d.shelf ? `${d.shelf.name} · ${t('shelfEdit.heading')}` : t('shelfEdit.heading');
		case '/(app)/series/[name]':
			return d.seriesName ?? null;
		case '/(app)/public/[owner]/[shelf]':
			return d.publicShelf?.name ?? null;
		case '/(app)/public/[owner]/[shelf]/[uuid]':
			return d.publicBook?.title ?? null;
	}
	const key = FIXED[route];
	return key ? t(key) : null;
}

export function pageTitle(route: string | null, data: Data): string {
	const app = t('app.name');
	const p = part(route, data);
	return p ? `${p} – ${app}` : app;
}
