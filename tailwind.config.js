const animate = require('tailwindcss-animate');

// Tailwind 3.4 emits no CSS for `bg-brand/10` when the colour is a bare var(), so the token
// goes through color-mix: <alpha-value> becomes the modifier (or 1 when there is none).
const tok = (name) =>
	`color-mix(in srgb, var(${name}) calc(<alpha-value> * 100%), transparent)`;

/** @type {import('tailwindcss').Config} */
module.exports = {
	theme: {
		container: {
			center: true,
			padding: {
				DEFAULT: '0.5rem',
				sm: '0.75rem',
				md: '1.25rem',
				lg: '1.5rem',
			},
			screens: {
				'2xl': '1400px',
			},
		},
		extend: {
			screens: {
				'xl-custom': '1080px',
			},
			fontFamily: {
				sans: ['"Exo 2"', 'Quicksand', 'sans-serif'],
				display: ['"Exo 2"', 'Quicksand', 'sans-serif'],
				serif: ['Instrument Serif', 'Georgia', 'serif'],
				mono: [
					'JetBrains Mono',
					'ui-monospace',
					'SF Mono',
					'Menlo',
					'monospace',
				],
			},
			colors: {
				'tmgr-blue': '#2c3e50',
				'tmgr-light-blue': '#0F67A7',
				'tmgr-gray': '#e2e2e2',

				// New design-system tokens (driven by CSS vars in _design-tokens.scss).
				// Renamed away from shadcn-claimed names (border/accent/card/...) to avoid
				// breaking existing shadcn-vue components.
				'surface-base': tok('--bg'),
				surface: tok('--bg-raised'),
				'surface-sunken': tok('--bg-sunken'),
				'surface-hover': tok('--bg-hover'),
				'surface-active': tok('--bg-active'),

				ink: tok('--fg'),
				'ink-muted': tok('--fg-muted'),
				'ink-subtle': tok('--fg-subtle'),
				'ink-faint': tok('--fg-faint'),

				line: tok('--line-color'),
				'line-strong': tok('--line-strong-color'),
				'line-focus': tok('--focus-border'),

				brand: tok('--brand-color'),
				'brand-bg': tok('--brand-bg-color'),
				'brand-fg': tok('--brand-fg-color'),
				'brand-hover': tok('--brand-hover-color'),
				'on-accent': tok('--on-accent-color'),

				'status-todo': tok('--s-todo'),
				'status-todo-bg': tok('--s-todo-bg'),
				'status-todo-fg': tok('--s-todo-fg'),
				'status-progress': tok('--s-progress'),
				'status-progress-bg': tok('--s-progress-bg'),
				'status-progress-fg': tok('--s-progress-fg'),
				'status-testing': tok('--s-testing'),
				'status-testing-bg': tok('--s-testing-bg'),
				'status-testing-fg': tok('--s-testing-fg'),
				'status-fix': tok('--s-fix'),
				'status-fix-bg': tok('--s-fix-bg'),
				'status-fix-fg': tok('--s-fix-fg'),
				'status-done': tok('--s-done'),
				'status-done-bg': tok('--s-done-bg'),
				'status-done-fg': tok('--s-done-fg'),

				'tag-bg': tok('--tag-bg'),
				'tag-fg': tok('--tag-fg'),

				border: 'hsl(var(--border))',
				input: 'hsl(var(--input))',
				ring: 'hsl(var(--ring))',
				background: 'hsl(var(--background))',
				foreground: 'hsl(var(--foreground))',
				primary: {
					DEFAULT: 'hsl(var(--primary))',
					foreground: 'hsl(var(--primary-foreground))',
				},
				secondary: {
					DEFAULT: 'hsl(var(--secondary))',
					foreground: 'hsl(var(--secondary-foreground))',
				},
				destructive: {
					DEFAULT: 'hsl(var(--destructive))',
					foreground: 'hsl(var(--destructive-foreground))',
				},
				muted: {
					DEFAULT: 'hsl(var(--muted))',
					foreground: 'hsl(var(--muted-foreground))',
				},
				accent: {
					DEFAULT: 'hsl(var(--accent))',
					foreground: 'hsl(var(--accent-foreground))',
				},
				popover: {
					DEFAULT: 'hsl(var(--popover))',
					foreground: 'hsl(var(--popover-foreground))',
				},
				card: {
					DEFAULT: 'hsl(var(--card))',
					foreground: 'hsl(var(--card-foreground))',
				},
				sidebar: {
					DEFAULT: 'hsl(var(--sidebar-background))',
					foreground: 'hsl(var(--sidebar-foreground))',
					primary: 'hsl(var(--sidebar-primary))',
					'primary-foreground': 'hsl(var(--sidebar-primary-foreground))',
					accent: 'hsl(var(--sidebar-accent))',
					'accent-foreground': 'hsl(var(--sidebar-accent-foreground))',
					border: 'hsl(var(--sidebar-border))',
					ring: 'hsl(var(--sidebar-ring))',
				},
			},
			borderRadius: {
				xl: 'calc(var(--radius) + 4px)',
				lg: 'var(--radius)',
				md: 'calc(var(--radius) - 2px)',
				sm: 'calc(var(--radius) - 4px)',
				card: 'var(--radius-lg)',
				panel: 'var(--radius-xl)',
				pill: 'var(--radius-full)',
			},
			boxShadow: {
				'tmgr-xs': 'var(--shadow-xs)',
				'tmgr-sm': 'var(--shadow-sm)',
				'tmgr-md': 'var(--shadow-md)',
				'tmgr-lg': 'var(--shadow-lg)',
			},
			fontSize: {
				'2xs': ['var(--text-2xs)', { lineHeight: 'var(--leading-tight)' }],
			},
			keyframes: {
				'accordion-down': {
					from: { height: 0 },
					to: { height: 'var(--radix-accordion-content-height)' },
				},
				'accordion-up': {
					from: { height: 'var(--radix-accordion-content-height)' },
					to: { height: 0 },
				},
				'collapsible-down': {
					from: { height: 0 },
					to: { height: 'var(--radix-collapsible-content-height)' },
				},
				'collapsible-up': {
					from: { height: 'var(--radix-collapsible-content-height)' },
					to: { height: 0 },
				},
				'tmgr-pulse': {
					'0%, 100%': { opacity: 1 },
					'50%': { opacity: 0.3 },
				},
				'tmgr-fade-in': {
					from: { opacity: 0 },
					to: { opacity: 1 },
				},
				'tmgr-slide-in-right': {
					from: { opacity: 0, transform: 'translateX(20px)' },
					to: { opacity: 1, transform: 'translateX(0)' },
				},
			},
			animation: {
				'accordion-down': 'accordion-down 0.2s ease-out',
				'accordion-up': 'accordion-up 0.2s ease-out',
				'collapsible-down': 'collapsible-down 0.2s ease-in-out',
				'collapsible-up': 'collapsible-up 0.2s ease-in-out',
				'tmgr-pulse': 'tmgr-pulse 1.5s var(--ease) infinite',
				'tmgr-fade-in': 'tmgr-fade-in var(--d-med) var(--ease)',
				'tmgr-slide-in-right':
					'tmgr-slide-in-right var(--d-med) var(--ease-snappy)',
			},
		},
	},
	plugins: [animate],
};
