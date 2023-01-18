<template>
	<div class="flex items-center task-category-in-task">
		<router-link
			v-if="category.id"
			:to="{ name: 'ProjectCategoryChildrenList', params: { id: category.id } }"
			class="inline bg-gray-700 text-white py-1 px-2 rounded ml-2 leading-none text-base z-10"
		>
			{{ category.title }}
		</router-link>

		<a
			:href="`/${
				category ? 'project-categories/' + category.id + '/' : ''
			}tasks/create`"
			class="opacity-10 hover:opacity-100 ml-1 tc-hidden md:inline add-task-to-category-from-task-category z-10"
			title="Create a task of this category"
			@click.prevent="
				$store.commit('createTaskInProjectCategoryId', {
					projectCategoryId: category.id,
					statusId,
				})
			"
		>
			<span class="material-icons text-2xl leading-none"
				>add_circle_outline</span
			>
		</a>
	</div>
</template>

<script>
	export default {
		name: 'CategoryBadge',
		props: {
			category: {
				required: true,
				type: Object,
			},
			statusId: {
				required: false,
				type: Number,
				default: null,
			},
		},
		created() {},
	};
</script>
