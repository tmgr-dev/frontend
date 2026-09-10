import DOMPurify from 'dompurify';

export default function sanitizeHtml(html: string): string {
	return DOMPurify.sanitize(html);
}
