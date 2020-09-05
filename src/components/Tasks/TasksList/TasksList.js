import DropdownMenu from "../../UIElements/DropdownMenu";
import Navbar from "../../UIElements/Navbar";

export default {
    name: 'TasksList',
    components: {
        DropdownMenu,
        Navbar
    },
    props: [],
    data() {
        return {
            panel: false,
            isOpen: false,
            showDefaultList: false,
            h1: {
                CurrentTasksList: 'Current tasks',
                HiddenTasksList: 'Hidden tasks',
                ArchiveTasksList: 'Archive tasks'
            },
            tasks: []
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
        async stopCountdown (task) {
            await this.$axios.delete(`tasks/${task.id}/countdown`)
            this.loadTasks()
        },
        async startCountdown (task) {
            await this.$axios.post(`tasks/${task.id}/countdown`)
            this.loadTasks()
        },
        getTaskFormattedTime(task) {
            let hours = Math.floor(task.common_time / 3600)
            let minutes = Math.ceil((task.common_time % 3600) / 60)

            return `${hours > 0 ? hours + ' hour' + (hours > 1 ? 's' : '') : ''}  ${minutes} minute${minutes > 1 ? 's' : ''}`
        },
        async loadTasks () {
            const {data: {data}} = await this.$axios.get(this.getTasksIndexUrl())
            this.tasks = data
        },
        getTasksIndexUrl () {
            if (this.status) {
                return `tasks/status/${this.status}?all`
            }
            return 'tasks/current?all'
        },
        getActions (task) {
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
        async updateStatus(task, status) {
            try {
                await this.$axios.put(`/tasks/${task.id}/${status}`)
                this.loadTasks()
            } catch (e) {
                console.error(e.getMessage())
            }
        },
        capitalize (s) {
            if (typeof s !== 'string') return ''
            return s.charAt(0).toUpperCase() + s.slice(1)
        }
    }
}
