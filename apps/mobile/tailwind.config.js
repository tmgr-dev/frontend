/**
 * TMGR mobile — dark theme, tokens ported 1:1 from web (_design-tokens.scss, .dark).
 * Colors reference CSS variables defined in global.css (same raw oklch/hex as web) so
 * the browser resolves them identically — Tailwind does not convert oklch to hex/lab.
 * @type {import('tailwindcss').Config}
 */
module.exports = {
	content: ['./App.{js,jsx,ts,tsx}', './src/**/*.{js,jsx,ts,tsx}'],
	presets: [require('nativewind/preset')],
	theme: {
		extend: {
			colors: {
				'surface-base': 'var(--bg)',
				surface: 'var(--bg-raised)',
				'surface-sunken': 'var(--bg-sunken)',
				'surface-hover': 'var(--bg-hover)',
				'surface-active': 'var(--bg-active)',

				ink: 'var(--fg)',
				'ink-muted': 'var(--fg-muted)',
				'ink-subtle': 'var(--fg-subtle)',
				'ink-faint': 'var(--fg-faint)',

				line: 'var(--line-color)',
				'line-strong': 'var(--line-strong-color)',

				brand: 'var(--brand-color)',
				'brand-bg': 'var(--brand-bg-color)',
				'brand-fg': 'var(--brand-fg-color)',
				'brand-hover': 'var(--brand-hover-color)',

				'status-todo': 'var(--s-todo)',
				'status-todo-bg': 'var(--s-todo-bg)',
				'status-todo-fg': 'var(--s-todo-fg)',
				'status-progress': 'var(--s-progress)',
				'status-progress-bg': 'var(--s-progress-bg)',
				'status-progress-fg': 'var(--s-progress-fg)',
				'status-testing': 'var(--s-testing)',
				'status-testing-bg': 'var(--s-testing-bg)',
				'status-testing-fg': 'var(--s-testing-fg)',
				'status-fix': 'var(--s-fix)',
				'status-fix-bg': 'var(--s-fix-bg)',
				'status-fix-fg': 'var(--s-fix-fg)',
				'status-done': 'var(--s-done)',
				'status-done-bg': 'var(--s-done-bg)',
				'status-done-fg': 'var(--s-done-fg)',
			},
			borderRadius: {
				card: '16px',
				pill: '999px',
				md: '12px',
				sm: '8px',
			},
			fontSize: {
				'2xs': '11px',
			},
		},
	},
	plugins: [],
};
