import CategoryView from '../ProjectCategoryView/index'


export default {
  name: 'ProjectCategoryList',
  components: {
    CategoryView
  },
  props: [],
  data () {
    return {
        h1: 'Projects categories',
        isShowModal: false,
        categories: null,
        category: null,
        parentCategories: [],
        showLoader: true
    }
  },
  computed: {
    showChildren () {
        return this.getId()
    },
    id () {
        return this.getId()
    }
  },
  mounted () {
    this.loadCategories()
  },
  methods: {
    async loadCategories () {
        const {data: {data}} = await this.$axios.get(`project_categories/children/${this.showChildren ? this.id : '' }?all`)
        this.categories = data
        this.showLoader = false
        if (!this.showChildren) {
            return
        }
        const {data: {data: category}} = await this.$axios.get(`project_categories/${this.id}/with/parents`)
        this.category = category
        this.parentCategories = this.extractParents(category)
    },
    extractParents (category, parents = []) {
        if (!category.parent_category) {
            return parents.reverse()
        }
        parents.push(category.parent_category)
        return this.extractParents(category.parent_category, parents)
    },
    getId () {
        return this.$route.params.id
    }
  }
}


