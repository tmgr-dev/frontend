module.exports = {
	content: ['./src/index.template.html', './src/**/*.{vue,js,ts,jsx,tsx}'],
	darkMode: 'class',
	theme: {
		extend: {
			colors: {
				'tmgr-blue': '#282f33',
				'tmgr-gray': '#f1b100',
			},
		},
	},
	plugins: [],
};
