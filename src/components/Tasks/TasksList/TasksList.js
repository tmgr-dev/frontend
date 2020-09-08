import DropdownMenu from "../../UIElements/DropdownMenu";
import Navbar from "../../UIElements/Navbar";
import DotsLoader from "@/components/UIElements/DotsLoader";

export default {
  name: 'TasksList',
  components: {
    DropdownMenu,
    Navbar,
    DotsLoader
  },
  props: [],
  data() {
    return {
      panel: false,
      isOpen: false,
      showDefaultList: false,
      summaryTimeString: null,
      showLoader: true,
      h1: {
        CurrentTasksList: 'Current tasks',
        HiddenTasksList: 'Hidden tasks',
        ArchiveTasksList: 'Archive tasks'
      },
      tasks: [],
      dotsProps: {}
    }
  },
  computed: {
    status() {
      return this.$route.meta.status
    }
  },
  created() {
    const handleEscape = (e) => {
      if (e.key === 'Esc' || e.key === 'Escape') {
        this.isOpen = false
      }
    }
    document.addEventListener('keydown', handleEscape)
    this.$once('hook:beforeDestroy', () => {
      document.removeEventListener('keydown', handleEscape)
    })
    this.loadTasks()
  },
  methods: {
    async stopCountdown(task, dotId) {
      this.dotsProps[dotId] = true
      await this.$axios.delete(`tasks/${task.id}/countdown`)
      await this.loadTasks()
      this.dotsProps[dotId] = false
    },
    async startCountdown(task, dotId) {
      this.dotsProps[dotId] = true
      await this.$axios.post(`tasks/${task.id}/countdown`)
      await this.loadTasks()
      this.dotsProps[dotId] = false
    },
    getTaskFormattedTime(task) {
      const taskTime = task instanceof Object ? task.common_time : task
      let hours = Math.floor(taskTime / 3600)
      let minutes = Math.ceil((taskTime % 3600) / 60)

      return `${hours > 0 ? hours + ' hour' + (hours > 1 ? 's' : '') : ''}  ${minutes} minute${minutes > 1 ? 's' : ''}`
    },
    async loadTasks() {
      const {data: {data}} = await this.$axios.get(this.getTasksIndexUrl())
      this.setDotsProps(data)
      this.summaryTimeString = this.getTaskFormattedTime(data.reduce((summary, task) => task.common_time + summary, 0))
      this.tasks = data
      this.showLoader = false
    },
    setDotsProps (tasks) {
      tasks.forEach(task => {
        this.$set(this.dotsProps, `hide-${task.id}`, false)
        this.$set(this.dotsProps, `done-${task.id}`, false)
        this.$set(this.dotsProps, `start-${task.id}`, false)
        this.$set(this.dotsProps, `stop-${task.id}`, false)
        this.$set(this.dotsProps, `activate-${task.id}`, false)
      })
    },
    getTasksIndexUrl() {
      if (this.status) {
        return `tasks/status/${this.status}?all`
      }
      return 'tasks/current?all'
    },
    getActions(task) {
      let actions = [
        {
          click: () => {
            this.$router.push(`/tasks/${task.id}/edit`)
          },
          label: 'Edit'
        }
      ]
      actions = this.addActionItem(actions, this.getActionItem(task, 'active', 'Open again'))
      actions = this.addActionItem(actions, this.getActionItem(task, 'hidden', 'Hide'))
      actions = this.addActionItem(actions, this.getActionItem(task, 'done', 'Done'))
      return actions
    },
    addActionItem(actions, item) {
      if (!item) {
        return actions
      }
      actions.push(item)
      return actions
    },
    getActionItem(task, status, label) {
      if (status === this.status) {
        return null
      }
      return {
        click: () => {
          this.updateStatus(task, status)
        },
        label: label
      }
    },
    async updateStatus(task, status, dotId) {
      try {
        this.dotsProps[dotId] = true
        await this.$axios.put(`/tasks/${task.id}/${status}`)
        await this.loadTasks()
        this.dotsProps[dotId] = false
      } catch (e) {
        this.dotsProps[dotId] = false
        console.error(e.getMessage())
      }
    },
    capitalize(s) {
      if (typeof s !== 'string') return ''
      return s.charAt(0).toUpperCase() + s.slice(1)
    }
  }
}
