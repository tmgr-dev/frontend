<template>
	<div class="relative" :class="[isPage ? 'ml-auto' : 'ml-5']">
		<div class="flex flex-row-reverse" v-if="task.assignees?.length > 0">
			<add-assignee-button
				v-if="isActive"
				@showModal="showModal"
				:task="task"
				addButtonClass="group flex h-8 w-8 cursor-default cursor-pointer rounded-full border-2 border-dashed border-gray-500 dark:border-gray-400 hover:dark:border-gray-200"
			/>
			<div
				v-for="(assignee, i) in task.assignees"
				key="i"
				:class="[avatarsClass, { '-mr-2': i > 0 }]"
			>
				<assignee-avatar
					:assignee="assignee"
					@deleteAssign="deleteAssign"
					:isActiveDeleteAsign="isActiveDeleteAsign"
				/>
			</div>
		</div>
		<div v-else class="flex flex-row-reverse">
			<add-assignee-button
				v-if="isActive"
				:task="task"
				@showModal="showModal"
				addButtonClass="group flex h-8 w-8 cursor-default cursor-pointer rounded-full border-2 border-dashed border-gray-500 dark:border-gray-400 hover:dark:border-gray-200"
			/>
			<div :class="[avatarsClass, { '-mr-2': isActive }]">
				<assignee-avatar
					:assignee="task.user"
					:isActiveDeleteAsign="!isActiveDeleteAsign"
				/>
			</div>
		</div>
	</div>
</template>

<script>
	import AssigneeAvatar from 'src/components/general/AssigneeAvatar.vue';
	import AddAssigneeButton from 'src/components/general/AddAssigneeButton.vue';

	export default {
		name: 'AssigneeAvatars',
		components: { AddAssigneeButton, AssigneeAvatar },
		props: {
			task: {
				required: true,
				type: Object,
			},
			avatarsClass: {
				type: String,
				default: '',
			},
			isActive: {
				type: Boolean,
				required: true,
			},
		},
		data: () => ({
			isActiveDeleteAsign: true,
		}),
		computed: {
			isPage() {
				return (
					this.$route.name === 'TasksEdit' || this.$route.name === 'TasksCreate'
				);
			},
		},
		methods: {
			showModal() {
				this.$emit('showModal');
			},
			deleteAssign(id) {
				this.$emit('deleteAssign', id);
			},
		},
	};
</script>

<style scoped></style>
