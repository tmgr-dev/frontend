export default {
	data: () => ({
		isShowAlert: false,
		alertTitle: '',
		alertDescription: ''
	}),
	methods: {
		showAlert (title = 'Saved', description = '') {
			this.isShowAlert = true
			this.alertTitle = title
			this.alertDescription = description
			setTimeout(() => this.isShowAlert = false, 3000)
		}
	}
}
