<template>
	<div v-if="isDesktop" data-tauri-drag-region class="flex items-center gap-1">
		<button
			v-for="control in controls"
			:key="control.action"
			type="button"
			:title="control.title"
			:class="[
				'flex h-7 w-7 items-center justify-center rounded-md text-tmgr-blue/60 transition-colors hover:bg-black/5 hover:text-tmgr-blue dark:text-tmgr-gray/60 dark:hover:bg-white/10 dark:hover:text-tmgr-gray',
				control.action === 'close' &&
					'hover:!bg-red-500 hover:!text-white dark:hover:!bg-red-500 dark:hover:!text-white',
			]"
			@click="run(control.action)"
		>
			<component :is="control.icon" class="h-3.5 w-3.5" :stroke-width="2.5" />
		</button>
	</div>
</template>

<script>
	import { isDesktopApp } from '@/utils/desktop';
	import { Maximize2, Minus, X } from 'lucide-vue-next';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'WindowControls',
		setup() {
			const controls = [
				{ action: 'close', title: 'Close', icon: X },
				{ action: 'minimize', title: 'Minimize', icon: Minus },
				{ action: 'toggleMaximize', title: 'Maximize', icon: Maximize2 },
			];

			const run = async (action) => {
				const { getCurrentWindow } = await import('@tauri-apps/api/window');
				await getCurrentWindow()[action]();
			};

			return { controls, run, isDesktop: isDesktopApp() };
		},
	});
</script>
