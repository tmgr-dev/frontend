export default {
	methods: {
		showAlert (title = 'Saved', description = '') {
			this.$refs.alert.show(title, description)
		}
	}
}
