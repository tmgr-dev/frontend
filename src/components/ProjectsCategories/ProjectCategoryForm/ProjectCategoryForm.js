
export default {
  name: 'ProjectCategoryForm',
  components: {},
  props: [],
  data() {
    return {
      h1: null,
      form: this.getDefaultForm(),
      parentCategories: null
    }
  },
  computed: {
    selected () {
      return !this.selectedParentCategory
    },
    isCreate () {
        return !this.$route.params.id && !this.form.id
    }
  },
  mounted() {
      this.setFormTexts()
      this.loadParentCategories()
      if (!this.isCreate) {
          this.loadModel()
      }
  },
  methods: {
    async loadModel () {
        const {data: {data}} = await this.$axios.get(`project_categories/${this.getId()}`)
        this.form = data
    },
    async loadParentCategories () {
        const {data: {data}} = await this.$axios.get('project_categories?all')
        this.parentCategories = data
    },
    async create (withRoutePush = true) {
        this.form.slug = this.generateSlug(this.form.title)
        if (!this.form.project_category_id) {
            delete this.form.project_category_id
        }
        const {data: {data}} = await this.$axios.post('project_categories', this.form)
        if (!withRoutePush) {
            return
        }
        this.$router.push({name: 'ProjectCategoryEdit', params: {id: data.id}})
    },
    async createAndContinue () {
        this.create(false)
        this.form = this.getDefaultForm()
    },
    generateSlug(text) {
      const ru = {
        'а': 'a', 'б': 'b', 'в': 'v', 'г': 'g', 'д': 'd',
        'е': 'e', 'ё': 'e', 'ж': 'j', 'з': 'z', 'и': 'i',
        'к': 'k', 'л': 'l', 'м': 'm', 'н': 'n', 'о': 'o',
        'п': 'p', 'р': 'r', 'с': 's', 'т': 't', 'у': 'u',
        'ф': 'f', 'х': 'h', 'ц': 'c', 'ч': 'ch', 'ш': 'sh',
        'щ': 'shch', 'ы': 'y', 'э': 'e', 'ю': 'u', 'я': 'ya', ' ': '-'
      }, n_str = [];

      text = text.replace(/[ъь]+/g, '').replace(/й/g, 'i');

      for (var i = 0; i < text.length; ++i) {
        n_str.push(
          ru[text[i]]
          || ru[text[i].toLowerCase()] == undefined && text[i]
          || ru[text[i].toLowerCase()].replace(/^(.)/, function (match) { return match.toUpperCase() })
        );
      }

      return n_str.join('').toLowerCase();
    },
    setFormTexts() {
        this.h1 = `${this.getFormTitlePrefix()} category`
    },
    getFormTitlePrefix () {
      return this.isCreate ? 'Add' : 'Edit'
    },
    getId () {
        return this.$route.params.id
    },
    getDefaultForm () {
        return {
            title: '',
            project_category_id: this.$route.params.project_category_id || null,
            slug: '',
        }
    }
  }
}


