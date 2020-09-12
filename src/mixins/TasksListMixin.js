export default {
  methods: {
    getTaskFormattedTime(task) {
      const taskTime = task instanceof Object ? task.common_time : task
      let hours = Math.floor(taskTime / 3600)
      let minutes = Math.ceil((taskTime % 3600) / 60)

      return `${hours > 0 ? hours + ' hour' + (hours > 1 ? 's' : '') : ''}  ${minutes} minute${minutes > 1 ? 's' : ''}`
    },
    getTasksIndexUrl() {
      if (this.status) {
        return `tasks/status/${this.status}?all`
      }
      return 'tasks/current?all'
    },
  }
}
