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
        parentCategories: [],
        statuses: [
            { value: 'created', name: `Created` },
            { value: 'active', name: `Active` },
            { value: 'done', name: `Done` }
        ],
        h1: {
            main: `${this.getId() ? 'Edit' : 'Create'} task`,
            CurrentTasksList: 'Current tasks',
            HiddenTasksList: 'Hidden tasks',
            ArchiveTasksList: 'Archive tasks'
        },
        selected: false,
        form: this.getDefaultForm(),
        showCountdown: true
    }
  },
  computed: {
    isCreate () {
        return !this.getId()
    }
  },
  mounted () {
    this.loadCategories()
    if (this.getId()) {
      this.loadModel()
    }
  },
  methods: {
    async loadCategories () {
        const {data: {data}} = await this.$axios.get('project_categories?all')
        this.parentCategories = data
    },
    async loadModel () {
        const {data: {data}} = await this.$axios.get(`tasks/${this.getId()}`)
        this.form = data
    },
    async toggleCountdown () {
        const {data: {data}} = await this.$axios[this.form.start_time ? 'delete' : 'post'](`tasks/${this.getId()}/countdown`)
        this.form = data
        this.$refs.countdown.$emit('update-task', this.form)
    },
    getId () {
        return this.$route.params.id
    },
    prepareForm() {
      if (this.form.project_category_id == '') {
          delete this.form.project_category_id
      }
    },
    async create () {
        this.prepareForm()
        const {data: {data}} = await this.$axios.post('tasks', this.form)
        this.$router.push({
            name: 'TasksEdit',
            params: {
                id: data.id
            }
        })
    },
    async save () {
        this.prepareForm()
        const {data: {data}} = await this.$axios.put(`tasks/${this.getId()}`, this.form)
        this.form = data
    },
    cancel () {
        console.log('cancel')
    },
    getDefaultForm () {
        return {
            title: '',
            status: 'active',
            project_category_id: '',
            description: ''
        };
    }
  }
}


