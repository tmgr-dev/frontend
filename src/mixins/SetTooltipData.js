export default {
	computed: {
		userSettings() {
			return this.$store.state.userSettings || {};
		},
	},
	methods: {
		setTooltipData(text) {
			return this.userSettings.showTooltips ? text : { visible: false };
		},
	},
};
