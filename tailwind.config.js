module.exports = {
	content: ['./src/index.template.html', './src/**/*.{vue,js,ts,jsx,tsx}'],
	darkMode: 'class',
	theme: {
		extend: {
			colors: {
				'tmgr-blue': '#2c3e50',
				'tmgr-gray': '#e2e2e2',
			},
		},
	},
	plugins: [],
};
