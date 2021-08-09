<template>
	<router-link
		v-if="category.id"
		:to="{name: 'ProjectCategoryChildrenList', params: {id: category.id}}"
		class="inline bg-gray-700 text-white py-1 px-2 rounded ml-2 leading-none text-base">
		{{ category.title }}
	</router-link>
	<router-link
		:to="`/${category ? 'project-categories/' + category.id + '/' : ''}tasks/create`" title="Add task to category"
		class="opacity-10 hover:opacity-100 tc-hidden md:inline add-task-to-category-from-task-category z-10">
		<span class="material-icons text-3xl -mt-1">add_circle_outline</span>
	</router-link>
</template>

<script>
export default {
	name: "CategoryBadge",
	props: {
		category: {
			required: true,
			type: Object
		}
	}
}
</script>
