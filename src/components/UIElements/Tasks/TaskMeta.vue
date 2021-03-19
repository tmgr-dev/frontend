<template>
	<div>
		<div class="flex">
			<router-link :to="`/${task.id}/edit`" class="font-bold text-xl z-10">
				{{ task.title }}
			</router-link>
			<div class="flex items-start task-category-in-task">
				<router-link
					v-if="task.category && showCategoryBadges"
					:to="{name: 'ProjectCategoryChildrenList', params: {id: task.category.id}}"
					class="inline bg-gray-700 text-white py-1 px-2 rounded ml-2 leading-none text-base">
					{{ task.category.title }}
				</router-link>
				<router-link
					:to="`/${task.category ? 'project-categories/' + task.category.id + '/' : ''}tasks/create`" title="Add task to category"
					class="opacity-25 hover:opacity-100 tc-hidden md:inline add-task-to-category-from-task-category z-10">
					<span class="material-icons text-3xl -mt-1">add_circle_outline</span>
				</router-link>
			</div>
		</div>
		<div class="flex items-start">
			<span>
				<span class="material-icons text-xl" :class="task.start_time ? 'text-green-600' : 'text-orange-600'">alarm</span>
			</span>
			<span class="text-gray-700 ml-2">{{ taskTime }}</span>
		</div>
	</div>
</template>

<script>
	export default {
		name: 'TaskMeta',
		props: {
			task: {
				type: Object,
				required: true
			},
			showCategoryBadges: {
				required: false,
				type: Boolean,
				default: true
			},
			taskTime: {
				type: String,
				required: true
			}
		}
	}
</script>
