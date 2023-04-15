<template>
	<div v-if="category.id" class="group flex items-center">
		<router-link
			:to="{ name: 'ProjectCategoryChildrenList', params: { id: category.id } }"
			class="z-10 rounded bg-gray-700 py-1 px-2 text-xs leading-none text-white lg:text-base"
		>
			{{ category.title }}
		</router-link>

		<a
			:href="`/${
				category ? 'project-categories/' + category.id + '/' : ''
			}tasks/create`"
			class="z-10 ml-1 hidden opacity-10 hover:opacity-100 group-hover:block"
			title="Create a task of this category"
			@click.prevent="
				$store.commit('createTaskInProjectCategoryId', {
					projectCategoryId: category.id,
					statusId,
				})
			"
		>
			<span class="material-icons text-2xl leading-none">
				add_circle_outline
			</span>
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
