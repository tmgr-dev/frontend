export default {
	methods: {
		setLoadingActions(tasks) {
			tasks.forEach(task => {
				this.isLoadingActions[`hide-${task.id}`] = false;
				this.isLoadingActions[`done-${task.id}`] = false;
				this.isLoadingActions[`start-${task.id}`] = false;
				this.isLoadingActions[`stop-${task.id}`] = false;
				this.isLoadingActions[`activate-${task.id}`] = false;
				this.isLoadingActions[`delete-${task.id}`] = false;
			});
		}
	}
};
