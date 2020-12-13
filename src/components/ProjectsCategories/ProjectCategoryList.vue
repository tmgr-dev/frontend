<template>
	<div>
		<BaseLayout>
			<template #header>{{ category ? category.title : h1 }}</template>
			<template #action>
				<breadcrumbs
					:current="category ? category.title : ''"
					:items="getBreadcrumbs()"
				/>
				<div class="md:absolute right-0 bottom-0 mr-5 mb-2">
					<router-link
						v-if="category" :to="`/projects-categories/${category.id}/edit`"
						title="Edit category name"
						class="pr-5 opacity-25 hover:opacity-100">
						<span class="material-icons text-4xl">edit</span>
					</router-link>
					<router-link
						:to="`/projects-categories/${id ? id + '/' : ''}create`" title="Add category"
						class="opacity-25 hover:opacity-100">
						<span class="material-icons text-4xl">add_circle_outline</span>
					</router-link>
				</div>
			</template>

			<template #body>
				<table v-if="showDefaultList" class="w-full shadow-lg rounded mt-5">
					<tbody :class="`${$color('blocks')}`">
					<tr v-for="category in categories"
							:key="category.id"
							:class="`cursor-pointer accordion border-b ${$color('borderMain')} hover:${$color('blocksHover')}`"
							@click="$router.push({name: 'ProjectCategoryChildrenList', params: {id: category.id}})">
						<td class="flex inline-flex items-center">
						<span class="py-3 w-40">
							<p :class="`${$color('textMain')}-800 text-sm`">{{ category.title }}</p>
							<p :class="`text-xs ${$color('textMain')}-600 font-medium`">Process</p>
						</span>
						</td>
						<td class="hidden md:table-cell">
							<p :class="`text-sm ${$color('textMain')}-700 font-medium`">Projects</p>
							<p :class="`text-xs ${$color('textMain')}-500 font-medium`">{{ category.children_count }}</p>
						</td>
						<td>
							<DropdownMenu class="lg:hidden" :actions="getActions(category)"></DropdownMenu>
							<div class="hidden lg:block">
								<router-link
									:to="{name: 'ProjectCategoryChildrenList', params: {id: category.id}}"
									class="rounded-l px-5 py-1 border border-4 border-green-400 text-green-600 hover:bg-green-400 hover:text-white">
									Open
								</router-link>
								<button
									class="rounded-r px-5 py-1 border border-4 border-red-400 text-red-600 hover:bg-red-400 hover:text-white"
									@click="deleteCategory(category)">
									Delete
								</button>
							</div>
						</td>
					</tr>
					</tbody>
				</table>
				<div v-else-if="categories && categories.length > 0">
					<div v-for="category in categories" class="w-full mt-2" :key="category.id">
						<div class="shadow-xl rounded-lg md:flex">
							<div class="w-full">
								<div class="p-4 md:p-5" :class="`${$color('blocks')} hover:${$color('blocksHover')}`">
									<div class="flex justify-between items-center">
										<div>
											<p
												class="font-bold text-xl cursor-pointer text-left pl-2"
												@click="$router.push({name: 'ProjectCategoryChildrenList', params: {id: category.id}})">
												{{ category.title }}</p>
											<div class="flex items-start">
											<span>
												<!-- <font-awesome-icon :class="task.start_time ? 'text-green-600' : 'text-orange-600'" icon="egg" class="text-2xl"/>-->
											</span>
												<span class="text-gray-700 ml-2">Projects: {{ category.children_count }}; Tasks: {{ category.tasks_count }}</span>
											</div>
										</div>
										<DropdownMenu class="lg:hidden" :actions="getActions(category)"></DropdownMenu>
										<div class="hidden lg:block">
											<new-button @click="$router.push({name: 'ProjectCategoryChildrenList', params: {id: category.id}})" class="mr-2">
												<span class="material-icons">open_in_new</span>
											</new-button>
											<new-button color="red" @click="deleteCategory(category)" class="mr-2">
												<span class="material-icons">delete</span>
											</new-button>
										</div>
									</div>
								</div>
							</div>
						</div>
					</div>
				</div>
				<div v-else-if="!showLoader" style="font-style: italic; font-size: 18px;" class="text-center">
					You don't have categories here
				</div>
				<div class="mt-5">
					<h1 class="text-3xl text-white-800 pt-5 relative text-left lg:text-center ml-2">
					<span>
							Tasks
							<a
								href="#"
								@click.prevent="selectAll"
								class="opacity-25 hover:opacity-100 inline md:hidden">
								<span class="material-icons text-3xl">done_all</span>
							</a>
							<router-link
								:to="`/${id ? 'project-categories/' + id + '/' : ''}tasks/create`" title="Add task to category"
								class="opacity-25 hover:opacity-100 inline md:hidden">
								<span class="material-icons text-3xl">add_circle_outline</span>
							</router-link>
					</span>
						<div class="md:absolute md:mt-2 md:mt-0 flex right-0 bottom-0 sm:mr-5">
							<div class="sm:mr-5 text-lg">
								<button type="button" :class="!status ? `` : `opacity-25 hover:opacity-100`" class="inline sm:mr-2 mr-1 bg-green-700 text-white p-2 rounded leading-none items-center" @click="$router.push(`/projects-categories/${id ? `${id}/children` : ''}`)">
									All
								</button>
								<button type="button" :class="status !== `current` ? `opacity-25 hover:opacity-100` : ``" class="inline sm:mr-2 sm:ml-2 mr-1 bg-blue-700 text-white p-2 rounded leading-none items-center" @click="!id ? $router.push({name: 'ProjectCategoryChildrenListWithStatus', params: {status: 'current'}}) : $router.push(`/projects-categories/${id ? `${id}/children` : '/status'}/current`)">
									Current
								</button>
								<button type="button" :class="status !== `hidden` ? `opacity-25 hover:opacity-100` : ``" class="inline sm:mr-2 sm:ml-2 mr-1 bg-gray-700 text-white p-2 rounded leading-none items-center" @click="!id ? $router.push({name: 'ProjectCategoryChildrenListWithStatus', params: {status: 'hidden'}}) : $router.push(`/projects-categories/${id ? `${id}/children` : '/status'}/hidden`)">
									Hidden
								</button>
								<button type="button" :class="status !== `done` ? `opacity-25 hover:opacity-100` : ``" class="inline sm:ml-2 mr-1 bg-red-700 text-white p-2 rounded leading-none items-center" @click="!id ? $router.push({name: 'ProjectCategoryChildrenListWithStatus', params: {status: 'done'}}) : $router.push(`/projects-categories/${id ? `${id}/children` : '/status'}/done`)">
									Archive
								</button>
							</div>
							<a
								href="#"
								@click.prevent="selectAll"
								alt="Select all"
								class="opacity-25 hover:opacity-100 hidden md:inline mr-2">
								<span class="material-icons text-3xl">done_all</span>
							</a>
							<router-link
								:to="`/${id ? 'project-categories/' + id + '/' : ''}tasks/create`" title="Add task to category"
								class="opacity-25 hover:opacity-100 hidden md:inline">
								<span class="material-icons text-4xl">add_circle_outline</span>
							</router-link>
						</div>
					</h1>
					<tasks-list-component
						v-if="tasks && tasks.length > 0"
						:tasks="tasks"
						@reload-tasks="loadTasks"
						:is-loading-actions="isLoadingActions"
						:use-task-status-for-buttons="true"
						:show-category-badges="false"
						ref="tasksListComponent"
					/>
					<div v-else style="font-style: italic; font-size: 18px;" class="mt-5 text-center">
						You don't have tasks in the category
					</div>
				</div>
				<loader v-if="showLoader" :is-active="true" style="margin-top: 2rem" />
			</template>
		</BaseLayout>
	</div>
</template>

<script>
import Breadcrumbs from '../UIElements/Breadcrumbs'
import TasksListComponent from "../UIElements/TasksListComponent";
import LoadingButtonActions from "@/mixins/LoadingButtonActions";
export default {
	name: 'ProjectCategoryList',
	components: {
		Breadcrumbs,
		TasksListComponent
	},
	mixins: [ LoadingButtonActions ],
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
			showLoader: true,
			isLoadingActions: {}
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
		await this.loadCategories()
		await this.loadTasks()
	},
	methods: {
		async loadTasks() {
			const {data: {data}} = await this.$axios.get(this.getTasksIndexUrl())
			this.setLoadingActions(data)
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
			await this.loadCategories()
		},
		extractParents(category, parents = []) {
			if (!category.parent_category) {
				return parents.reverse()
			}
			parents.push(category.parent_category)
			return this.extractParents(category.parent_category, parents)
		},
		selectAll () {
			if (!this.$refs.tasksListComponent) {
				return
			}
			this.$refs.tasksListComponent.selectAll()
		}
	}
}
</script>

<style lang="scss" scoped>
.my-adding {
	@media (max-width: 500px) {
		position: static;
		display: block;
		margin-left: auto;
		margin-right: 0;
	}
}
.actions-wrapper {
	.delete-button {
		margin-left: 90px;
	}
}
</style>
