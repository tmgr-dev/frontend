import { deleteTask, restoreDeletedTask } from 'src/actions/tmgr/tasks';

export default {
	methods: {
		setLoadingAction(dotId = null, actionStatus = true) {
			if (dotId !== null) {
				this.isLoadingActions[dotId] = actionStatus;
			}
		},
		async restoreTask(task, dotId = null) {
			try {
				this.setLoadingAction(dotId);
				task.deleted_at = await restoreDeletedTask(task.id);
			} catch (e) {
				console.error(e);
			} finally {
				this.setLoadingAction(dotId, false);
				this.confirm = undefined;
			}
		},
		async removeTask(task, dotId = null) {
			const deleteTaskConfirmation = async () => {
				try {
					this.setLoadingAction(dotId);
					task.deleted_at = await deleteTask(task.id);
				} catch (e) {
					console.error(e);
				} finally {
					this.setLoadingAction(dotId, false);
					this.confirm = undefined;
				}
			};
			this.showConfirm('Delete task', 'Are you sure?', deleteTaskConfirmation);
		},
	},
};
