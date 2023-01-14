export default (obj: any) => {
	const { app } = obj;

	app.config.globalProperties.showAlert = function(title = 'Saved', description = '') {
		this.$root.$refs.alert.show(title, description);
	};
}
