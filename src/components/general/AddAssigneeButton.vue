<template>
	<div
		:class="[addButtonClass, { '-ml-2': task.assignees?.length }]"
		v-tooltip.right="'Assign'"
		@click="showModal"
	>
		<span
			class="material-icons m-auto cursor-pointer text-base text-gray-600 dark:text-gray-200 dark:group-hover:text-white"
		>
			add
		</span>
	</div>
</template>

<script>
	export default {
		name: 'AddAssigneeButton',
		props: {
			task: {
				required: true,
				type: Object,
			},
			addButtonClass: {
				type: String,
				default: '',
			},
		},
		methods: {
			showModal() {
				this.$emit('showModal');
			},
		},
	};
</script>

<style scoped></style>
