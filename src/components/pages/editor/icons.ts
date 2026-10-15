const svg = (body: string, className = 'tmgr-icon') =>
	`<svg xmlns="http://www.w3.org/2000/svg" class="${className}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;

const heading = (digit: string) =>
	svg(`<path d="M4 12h8"/><path d="M4 18V6"/><path d="M12 18V6"/>${digit}`);

export const icons = {
	text: svg('<path d="M4 7V4h16v3"/><path d="M9 20h6"/><path d="M12 4v16"/>'),
	h1: heading('<path d="m17 12 3-2v8"/>'),
	h2: heading('<path d="M21 18h-4c0-4 4-3 4-6 0-1.5-2-2.5-4-1"/>'),
	h3: heading(
		'<path d="M17.5 10.5c1.7-1 3.5 0 3.5 1.5a2 2 0 0 1-2 2"/><path d="M17 17.5c2 1.5 4 .3 4-1.5a2 2 0 0 0-2-2"/>',
	),
	quote: svg(
		'<path d="M17 6H3"/><path d="M21 12H8"/><path d="M21 18H8"/><path d="M3 12v6"/>',
	),
	divider: svg('<path d="M5 12h14"/>'),
	bulletList: svg(
		'<path d="M3 12h.01"/><path d="M3 18h.01"/><path d="M3 6h.01"/><path d="M8 12h13"/><path d="M8 18h13"/><path d="M8 6h13"/>',
	),
	orderedList: svg(
		'<path d="M10 12h11"/><path d="M10 18h11"/><path d="M10 6h11"/><path d="M4 10h2"/><path d="M4 6h1v4"/><path d="M6 18H4c0-1 2-2 2-3s-1-1.5-2-1"/>',
	),
	taskList: svg(
		'<rect x="3" y="5" width="6" height="6" rx="1"/><path d="m3 17 2 2 4-4"/><path d="M13 6h8"/><path d="M13 12h8"/><path d="M13 18h8"/>',
	),
	code: svg('<path d="m16 18 6-6-6-6"/><path d="m8 6-6 6 6 6"/>'),
	table: svg(
		'<path d="M12 3v18"/><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18"/><path d="M3 15h18"/>',
	),
	image: svg(
		'<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>',
	),
	plus: svg('<path d="M5 12h14"/><path d="M12 5v14"/>'),
	grip: svg(
		'<circle cx="9" cy="12" r="1"/><circle cx="9" cy="5" r="1"/><circle cx="9" cy="19" r="1"/><circle cx="15" cy="12" r="1"/><circle cx="15" cy="5" r="1"/><circle cx="15" cy="19" r="1"/>',
	),
	bold: svg(
		'<path d="M6 12h9a4 4 0 0 1 0 8H7a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h7a4 4 0 0 1 0 8"/>',
	),
	italic: svg('<path d="M19 4h-9"/><path d="M14 20H5"/><path d="M15 4 9 20"/>'),
	strike: svg(
		'<path d="M16 4H9a3 3 0 0 0-2.83 4"/><path d="M14 12a4 4 0 0 1 0 8H6"/><path d="M4 12h16"/>',
	),
	link: svg(
		'<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>',
	),
	makeTask: svg(
		'<rect x="3" y="5" width="6" height="6" rx="1"/><path d="m3 17 2 2 4-4"/><path d="M13 6h8"/><path d="M13 12h8"/><path d="M13 18h8"/>',
	),
	bullet:
		'<svg xmlns="http://www.w3.org/2000/svg" class="tmgr-bullet" viewBox="0 0 24 24"><circle cx="12" cy="12" r="2.5"/></svg>',
	checked:
		'<svg xmlns="http://www.w3.org/2000/svg" class="tmgr-check" viewBox="0 0 24 24"><rect class="box" x="4" y="4" width="16" height="16" rx="4"/><path class="tick" d="m8.5 12.2 2.4 2.4 4.6-4.9"/></svg>',
	unchecked:
		'<svg xmlns="http://www.w3.org/2000/svg" class="tmgr-check" viewBox="0 0 24 24"><rect class="box" x="4.75" y="4.75" width="14.5" height="14.5" rx="3.5"/></svg>',
};
