export default {
	computed: {
		userSettings() {
			return this.$store.getters.getUserSettings || {};
		},
	},
	methods: {
		setTooltipData(text) {
			return this.userSettings.showTooltips ? text : { visible: false };
		},
	},
};
