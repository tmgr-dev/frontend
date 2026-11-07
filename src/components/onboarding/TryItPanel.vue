<template>
	<div
		v-if="visible"
		role="region"
		aria-label="Try it yourself"
		data-tour="ghost.make-it-yours"
		data-testid="try-it-panel"
		class="fixed right-4 bottom-4 z-40 w-72 max-w-[calc(100vw-2rem)] rounded-xl border border-violet-300 bg-violet-50 p-4 text-sm text-violet-900 shadow-lg dark:border-violet-500/30 dark:bg-violet-950 dark:text-violet-200"
	>
		<p class="font-semibold">Now it is your turn</p>
		<ul class="space-y-1 mt-2">
			<li
				v-for="item in items"
				:key="item.id"
				:data-testid="`try-it-${item.id}`"
				:data-done="done[item.id] ? 'true' : 'false'"
				class="flex items-center gap-2"
			>
				<span aria-hidden="true">{{ done[item.id] ? '✓' : '○' }}</span>
				<span :class="done[item.id] ? 'line-through opacity-70' : ''">{{
					item.label
				}}</span>
			</li>
		</ul>
		<p v-if="error" role="alert" class="mt-2 text-destructive">{{ error }}</p>
		<div class="mt-3 flex flex-wrap items-center gap-2">
			<Button
				size="sm"
				data-testid="create-my-workspace"
				:disabled="busy"
				@click="finish(false)"
			>
				Create my workspace
			</Button>
			<button
				type="button"
				data-testid="create-and-delete-demo"
				class="rounded-md px-2 py-1 underline underline-offset-4 hover:bg-violet-200 disabled:opacity-60 dark:hover:bg-violet-500/20"
				:disabled="busy"
				@click="finish(true)"
			>
				Create and delete the demo
			</button>
		</div>
	</div>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import { useDemoWorkspace } from '@/composable/useDemoWorkspace';
	import { completeDemoEntry } from '@/local/demoApi';
	import { ghostPlaying } from '@/onboarding/ghost';
	import { ghostStage } from '@/onboarding/ghostStage';
	import { tourBus } from '@/tours/events';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		reactive,
		ref,
	} from 'vue';

	const ITEMS = [
		{ id: 'move', label: 'Move a card to another column' },
		{ id: 'timer', label: 'Start a timer on a card' },
		{ id: 'add', label: 'Add a task of your own' },
	];

	export default defineComponent({
		name: 'TryItPanel',
		components: { Button },
		setup() {
			const { isDemo } = useDemoWorkspace();
			const busy = ref(false);
			const error = ref('');
			const done = reactive<Record<string, boolean>>({
				move: false,
				timer: false,
				add: false,
			});
			const visible = computed(
				() =>
					isDemo.value && !ghostPlaying.value && ghostStage.value === 'wheel',
			);

			const offs = [
				tourBus.on('task.status_changed', () => (done.move = true)),
				tourBus.on('timer.started', () => (done.timer = true)),
				tourBus.on('task.created', () => (done.add = true)),
			];
			onBeforeUnmount(() => offs.forEach((off) => off()));

			const finish = async (deleteDemo: boolean) => {
				if (busy.value) return;
				busy.value = true;
				error.value = '';
				try {
					await completeDemoEntry({ deleteDemo });
				} catch (e: any) {
					error.value = e?.message || 'Could not create the workspace.';
				} finally {
					busy.value = false;
				}
			};

			return { visible, items: ITEMS, done, busy, error, finish };
		},
	});
</script>
