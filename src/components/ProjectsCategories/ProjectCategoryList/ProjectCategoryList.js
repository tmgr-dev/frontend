import CategoryView from '../ProjectCategoryView/index'
import Breadcrumbs from '../../UIElements/Breadcrumbs'
import TasksListComponent from "../../UIElements/TasksListComponent";

export default {
  name: 'ProjectCategoryList',
  components: {
    CategoryView,
    Breadcrumbs,
    TasksListComponent
  },
  props: [],
  data() {
    return {
      h1: 'Projects categories',
      isShowModal: false,
      cardOpen: true,
      item: true,
      tasks: null,
      showDefaultList: false,
      categories: null,
      category: null,
      parentCategories: [],
      showLoader: true
    }
  },
  computed: {
    id() {
      return this.$route.params.id || ''
    },
    status() {
      return this.$route.params.status || ''
    }
  },
  async mounted() {
    console.log('test')
    await this.loadCategories()
    await this.loadTasks()
  },
  methods: {
    async loadTasks() {
      const {data: {data}} = await this.$axios.get(this.getTasksIndexUrl())
      this.tasks = data
      this.showLoader = false
    },
    getTasksIndexUrl() {
      if (this.$route.params.status) {
        return `tasks/?all&project_category_id=${this.id}&status=${this.$route.params.status}`
      }
      return `tasks/?all&project_category_id=${this.id}`
    },
    getBreadcrumbs() {
      const items = [
        {
          label: 'Categories',
          to: '/projects-categories'
        }
      ]
      console.log(this.getProjectCategoriesBreadcrumbs())
      return [...items, ...this.getProjectCategoriesBreadcrumbs()]
    },
    getProjectCategoriesBreadcrumbs() {
      if (!this.parentCategories) {
        return []
      }
      const result = []
      this.parentCategories.map(item => result.push({
        label: item.title,
        to: `/projects-categories/${item ? item.id + '/children' : ''}`
      }))
      return result
    },
    getActions(category) {
      return [
        {
          click: () => {
            return this.$router.push({name: 'ProjectCategoryChildrenList', params: {id: category.id}})
          },
          label: 'Open'
        },
        {
          click: () => {
            this.deleteCategory(category)
          },
          label: 'Delete'
        }
      ]
    },
    async loadChildrenCategories () {
      const {data: {data: category}} = await this.$axios.get(`project_categories/${this.id}/with/parents`)
      this.category = category
      this.parentCategories = this.extractParents(category)
    },
    async loadCategories() {
      const {data: {data}} = await this.$axios.get(`project_categories/children/${this.id}?all`)
      if (this.id) {
        await this.loadChildrenCategories()
      }
      this.categories = data
      this.showLoader = false
    },
    async deleteCategory(category) {
      await this.$axios.delete(`project_categories/${category.id}`)
      this.loadCategories()
    },
    extractParents(category, parents = []) {
      if (!category.parent_category) {
        return parents.reverse()
      }
      parents.push(category.parent_category)
      return this.extractParents(category.parent_category, parents)
    }
  }
}


