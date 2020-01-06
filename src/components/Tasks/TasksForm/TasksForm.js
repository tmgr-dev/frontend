export default {
  name: 'TasksForm',
  components: {},
  props: [],
  data () {
    return {
        panel: false,
        isOpen: false,
        counter: {
            common_time: 0,
            start_time: 0
        },
        parentCategories: [
            { id: 1, name: `Petya's projects` },
            { id: 2, name: `My projects` }
        ],
        h1: {
            CurrentTasksList: 'Current tasks',
            HiddenTasksList: 'Hidden tasks',
            ArchiveTasksList: 'Archive tasks'
        }
    }
  },
  computed: {

  },
  mounted () {

  },
  methods: {

  }
}


