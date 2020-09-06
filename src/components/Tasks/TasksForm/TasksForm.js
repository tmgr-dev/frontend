export default {
  name: 'TasksForm',
  components: {},
  props: [],
  data() {
    return {
      errors: {},
      showSaveAlert: false,
      panel: false,
      isOpen: false,
      checkpointUpdateKey: 0,
      counter: {
        common_time: 0,
        start_time: 0
      },
      parentCategories: [],
      statuses: [
        {value: 'created', name: `Created`},
        {value: 'active', name: `Active`},
        {value: 'done', name: `Done`}
      ],
      h1: {
        main: `${this.getId() ? 'Edit' : 'Create'} task`,
        CurrentTasksList: 'Current tasks',
        HiddenTasksList: 'Hidden tasks',
        ArchiveTasksList: 'Archive tasks'
      },
      checkpoints: [
        {
          description: 'Research subjects for task',
          start: 0,
          end: 156
        },
        {
          description: 'Second task',
          start: 156,
          end: 300
        },
        {
          description: 'Third task',
          start: 300,
          end: 750
        }
      ],
      selected: false,
      form: this.getDefaultForm(),
      showCountdown: true
    }
  },
  computed: {
    isCreate() {
      return !this.getId()
    }
  },
  mounted() {
    this.loadCategories()
    if (this.getId()) {
      this.loadModel()

      window.addEventListener("keydown", (e) => {
        if (e.ctrlKey && (e.key.toLowerCase() === 's' || e.key.toLowerCase() === 'ы')) {
          e.preventDefault();
          this.save()
        }
      })
    }
  },
  methods: {
    getActions() {
      return [
        {
          click: () => {
            this[this.form.id ? 'save' : 'create']()
          },
          label: this.form.id ? 'Save' : 'Create'
        }
      ]
    },
    async loadCategories() {
      const {data: {data}} = await this.$axios.get('project_categories?all')
      this.parentCategories = data
    },
    async loadModel() {
      const {data: {data}} = await this.$axios.get(`tasks/${this.getId()}`)
      data.common_time = data.common_time ? data.common_time : 0
      this.form = data
    },
    async toggleCountdown() {
      const {data: {data}} = await this.$axios[this.form.start_time ? 'delete' : 'post'](`tasks/${this.getId()}/countdown`)
      this.form = data
      this.$refs.countdown.$emit('update-task', this.form)
    },
    getId() {
      return this.$route.params.id
    },
    prepareForm() {
      if (this.form.project_category_id == '') {
        delete this.form.project_category_id
      }
    },
    async create() {
      try {
        this.prepareForm()
        const {data: {data}} = await this.$axios.post('tasks', this.form)
        this.$router.push({
          name: 'TasksEdit',
          params: {
            id: data.id
          }
        })
        this.showSavedAlert()
      } catch (e) {
        console.log(e)
        if (e.response && e.response && e.response.data.errors) {
          this.errors = e.response.data.errors
        }
      }
    },
    showSavedAlert() {
      this.showSaveAlert = true
      setTimeout(() => this.showSaveAlert = false, 3000)
    },
    async save() {
      try {
        this.prepareForm()
        const {data: {data}} = await this.$axios.put(`tasks/${this.getId()}`, this.form)
        this.form = data
        this.showSavedAlert()
      } catch (e) {
        if (e.response && e.response && e.response.data.errors) {
          this.errors = e.response.data.errors
        }
      }
    },
    cancel() {
      window.history.back();
    },
    getDefaultForm() {
      return {
        title: '',
        status: 'active',
        project_category_id: '',
        description: ''
      };
    },
    secondsToStringTime(seconds) {
      const second = seconds % 60
      let minute = (seconds - second) / 60 | 0
      const hour = minute / 60 | 0
      minute = minute - (hour * 60)

      return `${this.prepareClockNumber(hour)}:${this.prepareClockNumber(minute)}:${this.prepareClockNumber(second)}`
    },
    stringTimeToSeconds(stringTime) {
      let times = stringTime.split(':')
      times = times.map(parseInt)
      return times[0] * 60 * 60 + times[1] * 60 + times[2]
    },
    prepareClockNumber(num) {
      return num < 10 ? '0' + num : num
    },
    addCheckpoint() {
      if (!this.form.checkpoints) {
        this.form.checkpoints = []
      }
      const currentTime = this.form.common_time
      if (this.form.checkpoints.length !== 0 && this.form.checkpoints.length !== 1) {
        this.form.checkpoints[this.form.checkpoints.length - 1].end = currentTime
      }
      this.form.checkpoints.push({
        description: 'New one',
        start: currentTime,
        end: currentTime
      })
      ++this.checkpointUpdateKey
      console.log(this.form.checkpoints)
    },
    deleteCheckpoint(checkpointIndex) {
      this.form.checkpoints.splice(checkpointIndex, 1)
      ++this.checkpointUpdateKey
    },
    updateSeconds(seconds) {
      if (!this.form.checkpoints || this.form.checkpoints.length === 0) {
        return
      }
      this.form.checkpoints[this.form.checkpoints.length - 1].end = seconds
    }
  }
}


