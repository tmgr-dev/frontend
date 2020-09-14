export default {
  methods: {
    setLoadingActions (tasks) {
      tasks.forEach(task => {
        this.$set(this.isLoadingActions, `hide-${task.id}`, false)
        this.$set(this.isLoadingActions, `done-${task.id}`, false)
        this.$set(this.isLoadingActions, `start-${task.id}`, false)
        this.$set(this.isLoadingActions, `stop-${task.id}`, false)
        this.$set(this.isLoadingActions, `activate-${task.id}`, false)
        this.$set(this.isLoadingActions, `delete-${task.id}`, false)
      })
    }
  }
}
