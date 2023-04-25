module.exports = {
	content: ['./src/index.template.html', './src/**/*.{vue,js,ts,jsx,tsx}'],
	darkMode: 'class',
	theme: {
		extend: {
			fontFamily: {
				sans: ['Quicksand', 'sans-serif'],
			},
			colors: {
				'tmgr-blue': '#2c3e50',
				'tmgr-gray': '#e2e2e2',
			},
			minHeight: {
				filters: '62px',
			},
		},
	},
	plugins: [],
};
