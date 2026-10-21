const filterModule = {
	state: () => ({
		selectedCategory: 0,
		searchText: null,
		selectedUser: 0,
		selectedPersona: '',
		selectedPriorities: [],
		prioritySort: false,
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
		updateSelectedPriorities(state, data) {
			state.selectedPriorities = data;
		},
		updatePrioritySort(state, data) {
			state.prioritySort = data;
		},
	},
};

export default filterModule;
