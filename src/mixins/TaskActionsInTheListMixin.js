export default {
	methods: {
		setLoadingAction(dotId = null, actionStatus = true) {
			if (dotId !== null) {
				this.isLoadingActions[dotId] = actionStatus
			}
		},
		async restoreTask (task, dotId = null) {
			try {
				this.setLoadingAction(dotId)
				const {data: {data}} = await this.$axios.post(`/tasks/${task.id}/restore`)
				task.deleted_at = data.deleted_at
			} catch (e) {
				console.error(e)
			} finally {
				this.setLoadingAction(dotId, false)
				this.confirm = undefined
			}
		},
		async deleteTask (task, dotId = null, loadTasks = true) {
			const deleteTask = async () => {
				try {
					this.setLoadingAction(dotId)
					const {data: {data}} = await this.$axios.delete(`/tasks/${task.id}`)
					task.deleted_at = data.deleted_at
					// if (loadTasks) {
					// 	await this.loadTasks()
					// }
				} catch (e) {
					console.error(e)
				} finally {
					this.setLoadingAction(dotId, false)
					this.confirm = undefined
				}
			}
			this.showConfirm('Delete task', 'Are you sure?', deleteTask)
		}
	}
}
