import DropdownMenu from "../../UIElements/DropdownMenu";
export default {
  name: 'tasks-list',
  components: {
      DropdownMenu
  },
  props: [],
  data () {
    return {
        panel: false,
        isOpen: false
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
    },
  mounted () {

  },
  methods: {

  }
}


