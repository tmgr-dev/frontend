<template>
	<span
		role="link"
		tabindex="0"
		draggable="false"
		:aria-label="label ? `Open ${label}` : 'Open member'"
		class="hover:ring-brand/40 dark:hover:ring-brand/60 inline-flex cursor-pointer rounded-full outline-none transition-shadow hover:ring-2"
		@click.stop.prevent="open"
		@keydown.enter.stop.prevent="open"
	>
		<slot />
	</span>
</template>

<script lang="ts">
	import { computed, defineComponent } from 'vue';
	import { useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'MemberAvatarLink',
		props: {
			userId: { type: Number, required: true },
			label: { type: String, default: '' },
		},
		setup(props) {
			const store = useStore();
			const router = useRouter();
			const workspaceCode = computed(
				() => store.getters.currentWorkspace?.code || '',
			);

			const open = () => {
				if (!props.userId || !workspaceCode.value) return;
				if (store.state.currentTaskIdForModal != null) {
					store.commit('closeTaskModal');
				}
				router.push(`/${workspaceCode.value}/team/${props.userId}`);
			};

			return { open };
		},
	});
</script>
