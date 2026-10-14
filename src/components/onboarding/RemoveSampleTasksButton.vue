<template>
	<button
		v-if="visible"
		type="button"
		class="min-h-[44px] rounded border border-line px-3 text-sm text-ink-muted hover:bg-surface-hover hover:text-ink disabled:opacity-50 sm:min-h-[32px]"
		data-testid="remove-sample-tasks"
		:disabled="busy"
		@click="remove"
	>
		Remove sample tasks
	</button>
</template>

<script lang="ts">
	import { toast } from '@/components/ui/toast';
	import { showRemoveStarters, STARTERS_EXPECTED_KEY } from '@/onboarding/starters';
	import { onboardingApi } from '@/tours/api';
	import { useTours } from '@/tours/useTours';
	import { requestCache } from '@/utils/requestCache';
	import { computed, defineComponent, ref, type PropType } from 'vue';
	import { useStore } from 'vuex';

	const flagKey = (uid: unknown) => `tmgr:onboarding:starters-removed:${uid}`;

	const readFlag = (uid: unknown): boolean => {
		try {
			return localStorage.getItem(flagKey(uid)) === '1';
		} catch {
			return false;
		}
	};

	const readExpected = (): boolean => {
		try {
			return Number(localStorage.getItem(STARTERS_EXPECTED_KEY)) > 0;
		} catch {
			return false;
		}
	};

	export default defineComponent({
		name: 'RemoveSampleTasksButton',
		props: {
			tasks: { type: Array as PropType<Array<{ settings?: Array<{ key?: string }> }> | null>, default: null },
		},
		emits: ['removed'],
		setup(props, { emit }) {
			const store = useStore();
			const tours = useTours();
			const busy = ref(false);
			const removedFlag = ref(false);

			const userId = computed(() => store.state.user?.id ?? null);
			const intent = computed(() => tours.onboarding.value?.intent ?? null);
			const hasStarter = computed(
				() =>
					!!props.tasks?.some((task) =>
						task.settings?.some((setting) => setting.key === 'starter'),
					),
			);
			const visible = computed(() =>
				showRemoveStarters({
					intent: intent.value,
					hasUser: userId.value !== null,
					removed: removedFlag.value,
					boardTasks: props.tasks !== null,
					boardHasStarter: hasStarter.value,
					expected: readExpected() && !readFlag(userId.value),
				}),
			);

			const remove = async () => {
				busy.value = true;
				try {
					const { removed } = await onboardingApi().removeStarterTasks();
					try {
						localStorage.setItem(flagKey(userId.value), '1');
					} catch {}
					removedFlag.value = true;
					try {
						localStorage.setItem(STARTERS_EXPECTED_KEY, '');
					} catch {}
					if (removed === 0) {
						toast({ title: 'No sample tasks to remove' });
					} else {
						requestCache.invalidate(/^tasks-status-/);
						requestCache.invalidate(/^task-/);
						toast({
							title: `Removed ${removed} sample ${removed === 1 ? 'task' : 'tasks'}`,
						});
						emit('removed', removed);
					}
				} catch (error) {
					console.error(error);
					toast({ title: 'Could not remove sample tasks', variant: 'destructive' });
				} finally {
					busy.value = false;
				}
			};

			return { visible, busy, remove };
		},
	});
</script>
