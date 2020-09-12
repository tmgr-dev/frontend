import DropdownMenu from "../../UIElements/DropdownMenu";
import Navbar from "../../UIElements/Navbar";
import TasksListComponent from "../../UIElements/TasksListComponent";
import DefaultTasksListComponent from "@/components/UIElements/DefaultTasksListComponent";
import LoadingButtonActions from "@/mixins/LoadingButtonActions";
import TasksListMixin from "@/mixins/TasksListMixin";

export default {
  name: 'TasksList',
  components: {
    DropdownMenu,
    Navbar,
    TasksListComponent,
    DefaultTasksListComponent
  },
  mixins: [ LoadingButtonActions, TasksListMixin ],
  data() {
    return {
      panel: false,
      showDefaultList: false,
      summaryTimeString: null,
      showLoader: true,
      h1: {
        CurrentTasksList: 'Current tasks',
        HiddenTasksList: 'Hidden tasks',
        ArchiveTasksList: 'Archive tasks'
      },
      tasks: [],
      isLoadingActions: {}
    }
  },
  computed: {
    status() {
      return this.$route.meta.status
    }
  },
  async created() {
    const data = await this.loadTasks()
    this.setLoadingActions(data)
  },
  methods: {
    async loadTasks() {
      const {data: {data}} = await this.$axios.get(this.getTasksIndexUrl())
      this.summaryTimeString = this.getTaskFormattedTime(data.reduce((summary, task) => task.common_time + summary, 0))
      this.tasks = data
      this.showLoader = false
      return data
    }
  }
}
