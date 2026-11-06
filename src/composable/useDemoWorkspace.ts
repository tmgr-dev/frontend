import {
	activeLocalWorkspace,
	onActiveLocalWorkspaceChange,
} from '@/local/runtime';
import { computed, onBeforeUnmount, ref } from 'vue';

export function useDemoWorkspace() {
	const active = ref(activeLocalWorkspace());
	const unsubscribe = onActiveLocalWorkspaceChange(() => {
		active.value = activeLocalWorkspace();
	});
	onBeforeUnmount(unsubscribe);

	const isDemo = computed(() => active.value?.demo === true);
	const code = computed(() => active.value?.code ?? null);

	return { isDemo, code };
}
