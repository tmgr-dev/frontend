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
            h1: {
                CurrentTasksList: 'Current tasks',
                HiddenTasksList: 'Hidden tasks',
                ArchiveTasksList: 'Archive tasks'
            },
            tasks: []
        }
    },
    computed: {

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
    mounted() {
        window.console.log(this.$route)
    },
    methods: {
        async loadTasks () {
            const {data: {data}} = await this.$axios.get('tasks/current?all')
            this.tasks = data
        },
        capitalize (s) {
            if (typeof s !== 'string') return ''
            return s.charAt(0).toUpperCase() + s.slice(1)
        }
    }
}
