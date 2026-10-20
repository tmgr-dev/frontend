<template>
	<ShareMenuItems
		v-if="links"
		:web="links.web"
		:app="links.app"
		:markdown="() => links.markdown"
		:submenu="submenu"
		:open-in-app="openInApp"
		test-id-prefix="task-share"
	/>
</template>

<script>
	import ShareMenuItems from '@/components/share/ShareMenuItems.vue';
	import store from '@/store';
	import { taskShareLinks } from '@/utils/taskLinks';
	import { computed, defineComponent } from 'vue';

	export default defineComponent({
		name: 'TaskShareItems',
		components: { ShareMenuItems },
		props: {
			task: { type: Object, required: true },
			submenu: { type: Boolean, default: false },
			openInApp: { type: Boolean, default: false },
		},
		setup(props) {
			const workspace = computed(
				() =>
					(store.state.workspaces || []).find(
						(w) =>
							props.task.workspace_id != null &&
							Number(w.id) === Number(props.task.workspace_id),
					) ?? store.getters.currentWorkspace,
			);

			const links = computed(() =>
				props.task.id && workspace.value?.code
					? taskShareLinks(props.task, workspace.value)
					: null,
			);

			return { links };
		},
	});
</script>
