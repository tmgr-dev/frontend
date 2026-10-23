<template>
	<div v-if="isDesktop" data-tauri-drag-region class="flex items-center gap-1">
		<button
			v-for="control in controls"
			:key="control.action"
			type="button"
			:title="control.title"
			:class="[
				'flex h-7 w-7 items-center justify-center rounded-md text-ink-subtle transition-colors hover:bg-surface-hover hover:text-ink',
				control.action === 'close' &&
					'hover:!bg-destructive hover:!text-destructive-foreground',
			]"
			@click="run(control.action)"
		>
			<component :is="control.icon" class="h-3.5 w-3.5" :stroke-width="2.5" />
		</button>
	</div>
</template>

<script>
	import { hasNativeTitleBar, isDesktopApp } from '@/utils/desktop';
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

			return {
				controls,
				run,
				isDesktop: isDesktopApp() && !hasNativeTitleBar(),
			};
		},
	});
</script>
