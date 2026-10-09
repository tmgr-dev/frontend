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
				<div
					class="pointer-events-none select-none opacity-40 blur-[2px] filter"
				>
					<slot name="preview">
						<slot />
					</slot>
				</div>

				<!-- Overlay with enable CTA -->
				<div
					class="absolute inset-0 flex items-center justify-center bg-gray-900/30 backdrop-blur-sm dark:bg-gray-950/50"
				>
					<div
						class="mx-4 max-w-md rounded-xl border border-gray-200 bg-white p-8 text-center shadow-2xl dark:border-gray-700 dark:bg-gray-800"
					>
						<!-- Icon -->
						<div
							class="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-blue-100 dark:bg-blue-900/30"
						>
							<component
								:is="iconComponent"
								class="h-8 w-8 text-blue-600 dark:text-blue-400"
							/>
						</div>

						<!-- Title -->
						<h2 class="mb-2 text-xl font-bold text-gray-900 dark:text-white">
							{{ title }}
						</h2>

						<!-- Description -->
						<p class="mb-6 text-gray-600 dark:text-gray-400">
							{{ description }}
						</p>

						<router-link
							v-if="copy.kind === 'link'"
							:to="copy.to"
							class="inline-flex items-center justify-center rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 dark:bg-blue-500 dark:hover:bg-blue-600"
						>
							{{ copy.text }}
						</router-link>
						<p v-else class="text-sm text-gray-500 dark:text-gray-400">
							{{ copy.text }}
						</p>
					</div>
				</div>
			</div>
		</template>
	</div>
</template>

<script setup lang="ts">
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { gateCopy } from '@/utils/modules';
	import { Lock } from 'lucide-vue-next';
	import { computed, type Component } from 'vue';
	import { useStore } from 'vuex';

	interface Props {
		featureKey: string;
		title: string;
		description: string;
		icon?: Component;
	}

	const props = defineProps<Props>();

	const store = useStore();
	const { isFeatureEnabled } = useFeatureToggles();

	const iconComponent = computed(() => props.icon || Lock);

	const isEnabled = computed(() => isFeatureEnabled(props.featureKey));

	const copy = computed(() =>
		gateCopy(
			store.getters['featureToggles/canManageModules'],
			store.getters['featureToggles/isHiddenByMe'](props.featureKey),
		),
	);
</script>
