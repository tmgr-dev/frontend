<template>
	<div class="relative" :class="[isModal ? 'ml-5' : 'ml-auto']">
		<div class="flex flex-row-reverse">
			<div
				v-if="showAssigneeControls"
				class="group flex h-8 w-8 cursor-default cursor-pointer rounded-full border-2 border-dashed border-gray-500 dark:border-gray-400 hover:dark:border-gray-200"
				:class="{ '-ml-2': assignees?.length }"
				v-tooltip.right="'Assign'"
				@click="$emit('showModal')"
			>
				<span
					class="material-icons m-auto cursor-pointer text-base text-gray-600 dark:text-gray-200 dark:group-hover:text-white"
				>
					add
				</span>
			</div>

			<template v-if="assignees?.length > 0">
				<div
					v-for="(assignee, i) in assignees"
					:key="assignee.id"
					:class="[
						'group relative flex shrink-0 cursor-default rounded-full border-green-400 bg-green-600 shadow shadow-neutral-300',
						avatarsClass,
						{ '-mr-2': i > 0 },
					]"
				>
					<!--	probably we don't need even this component. The initial idea was to create AssigneeUsers component				-->
					<assignee-avatar
						:assignee="assignee"
						:show-removing-assignee="showAssigneeControls"
						@deleteAssign="(id) => $emit('deleteAssignee', id)"
					/>
				</div>
			</template>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { computed } from 'vue';
	import AssigneeAvatar from 'src/components/general/AssigneeAvatar.vue';
	interface assigneeType {
		name: string;
		id: number;
	}
	interface Props {
		assignees: assigneeType[];
		avatarsClass: string;
		showAssigneeControls: Boolean;
		isModal: Boolean;
	}
	interface Data extends Props {}
	const props = defineProps<Props>();
	const emit = defineEmits(['showModal', 'deleteAssignee']);

	// const isPage = computed(() => {
	// 	return $route.name === 'TasksEdit' || $route.name === 'TasksCreate';
	// });

	// export default {
	// 	name: 'AssigneeUsers',
	// 	// emits: ['showModal', 'deleteAssignee'],
	// 	components: {
	// 		AssigneeAvatar,
	// 	},
	// 	props: {
	// 		assignees: {
	// 			type: Array,
	// 			required: true,
	// 			default: [],
	// 		},
	// 		avatarsClass: {
	// 			type: String,
	// 			default: '',
	// 		},
	// 		showAssigneeControls: {
	// 			type: Boolean,
	// 			default: true,
	// 		},
	// 	},
	// 	computed: {
	// 		/* @todo this component shouldn't know about it. isPage is needed only to change the css class, so the class should be passed from parent component */
	// 		isPage() {
	// 			return (
	// 				this.$route.name === 'TasksEdit' || this.$route.name === 'TasksCreate'
	// 			);
	// 		},
	// 	},
	// };
</script>
