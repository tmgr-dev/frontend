const filterModule = {
	state: () => ({
		selectedCategory: 0,
		searchText: null,
		selectedUser: 0,
		selectedPersona: '',
	}),
	mutations: {
		updateSelectedCategory(state, data) {
			state.selectedCategory = data;
		},
		updateSearchText(state, data) {
			state.searchText = data;
		},
		updateSelectedUser(state, data) {
			state.selectedUser = data;
		},
		updateSelectedPersona(state, data) {
			state.selectedPersona = data;
		},
	},
};

export default filterModule;
