import { marked } from 'marked';
import DOMPurify from 'dompurify';

export function renderMarkdown(source: string): string {
	const html = marked.parse(source, { async: false }) as string;
	return DOMPurify.sanitize(html);
}
