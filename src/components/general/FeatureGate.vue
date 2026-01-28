<template>
	<div class="relative">
		<!-- Feature enabled: show actual content -->
		<template v-if="isEnabled">
			<slot />
		</template>

		<!-- Feature disabled: show preview with overlay -->
		<template v-else>
			<div class="relative">
				<!-- Preview content with blur/fade -->
				<div class="pointer-events-none select-none opacity-40 blur-[2px] filter">
					<slot name="preview">
						<slot />
					</slot>
				</div>

				<!-- Overlay with enable CTA -->
				<div class="absolute inset-0 flex items-center justify-center bg-gray-900/30 backdrop-blur-sm dark:bg-gray-950/50">
					<div class="mx-4 max-w-md rounded-xl border border-gray-200 bg-white p-8 text-center shadow-2xl dark:border-gray-700 dark:bg-gray-800">
						<!-- Icon -->
						<div class="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-blue-100 dark:bg-blue-900/30">
							<component :is="iconComponent" class="h-8 w-8 text-blue-600 dark:text-blue-400" />
						</div>

						<!-- Title -->
						<h2 class="mb-2 text-xl font-bold text-gray-900 dark:text-white">
							{{ title }}
						</h2>

						<!-- Description -->
						<p class="mb-6 text-gray-600 dark:text-gray-400">
							{{ description }}
						</p>

						<!-- Toggle switch -->
						<div class="flex items-center justify-center gap-3">
							<span class="text-sm text-gray-500 dark:text-gray-400">Enable this feature</span>
							<label class="relative inline-flex cursor-pointer items-center">
								<input
									type="checkbox"
									:checked="false"
									@change="enableFeature"
									class="peer sr-only"
								/>
								<div class="peer h-7 w-14 rounded-full bg-gray-200 after:absolute after:left-[4px] after:top-[4px] after:h-5 after:w-5 after:rounded-full after:border after:border-gray-300 after:bg-white after:transition-all after:content-[''] peer-checked:bg-blue-600 peer-checked:after:translate-x-full peer-checked:after:border-white peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-blue-300 dark:border-gray-600 dark:bg-gray-700 dark:peer-focus:ring-blue-800"></div>
							</label>
						</div>

						<!-- Loading state -->
						<div v-if="isLoading" class="mt-4 flex items-center justify-center gap-2 text-sm text-blue-600">
							<svg class="h-4 w-4 animate-spin" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
								<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
								<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
							</svg>
							<span>Enabling feature...</span>
						</div>
					</div>
				</div>
			</div>
		</template>
	</div>
</template>

<script setup lang="ts">
import { ref, computed, type Component } from 'vue';
import { useStore } from 'vuex';
import { useFeatureToggles } from '@/composable/useFeatureToggles';
import { Lock } from 'lucide-vue-next';

interface Props {
	featureKey: string;
	title: string;
	description: string;
	icon?: Component;
}

const props = defineProps<Props>();

const store = useStore();
const { isFeatureEnabled } = useFeatureToggles();
const isLoading = ref(false);

const iconComponent = computed(() => props.icon || Lock);

const isEnabled = computed(() => isFeatureEnabled(props.featureKey));

const currentWorkspaceId = computed(() => {
	const setting = store.state.user?.settings?.find((s: any) => s.key === 'current_workspace');
	return setting?.value;
});

const enableFeature = async () => {
	isLoading.value = true;
	try {
		await store.dispatch('featureToggles/updateWorkspaceToggles', {
			workspaceId: currentWorkspaceId.value,
			toggles: { [props.featureKey]: true }
		});
		window.location.reload();
	} catch (error) {
		console.error('Failed to enable feature:', error);
	} finally {
		isLoading.value = false;
	}
};
</script>

