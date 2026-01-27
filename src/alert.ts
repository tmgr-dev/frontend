import { App } from 'vue';

export default (obj: { app: App }) => {
	const { app } = obj;

	app.config.globalProperties.showAlert = function (
		title = 'Saved',
		description = '',
	) {
		const rootInstance = this.$root;
		if (rootInstance && rootInstance.$refs && rootInstance.$refs.alert) {
			rootInstance.$refs.alert.show(title, description);
		} else {
			console.warn('Alert component not found');
		}
	};
};
