const filterModule = {
	state: () => ({
		selectedCategory: 0,
		searchText: null,
		selectedUser: 0,
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
	},
};

export default filterModule;
