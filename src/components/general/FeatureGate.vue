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
					class="absolute inset-0 flex items-center justify-center bg-background/60 backdrop-blur-sm"
				>
					<div
						class="mx-4 max-w-md rounded-xl border border-line bg-surface p-8 text-center shadow-tmgr-lg"
					>
						<!-- Icon -->
						<div
							class="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-brand-bg"
						>
							<component :is="iconComponent" class="h-8 w-8 text-brand" />
						</div>

						<!-- Title -->
						<h2 class="mb-2 text-xl font-bold text-ink">
							{{ title }}
						</h2>

						<RequiresAccount
							v-if="mode === 'requires-account'"
							class="!border-0 !bg-transparent !p-0"
						/>
						<p
							v-else-if="mode === 'not-in-local-workspace'"
							class="text-sm text-ink-muted"
						>
							Not available in local workspaces
						</p>
						<template v-else>
							<!-- Description -->
							<p class="mb-6 text-ink-muted">
								{{ description }}
							</p>

							<router-link
								v-if="copy.kind === 'link'"
								:to="copy.to"
								:class="buttonVariants()"
							>
								{{ copy.text }}
							</router-link>
							<p v-else class="text-sm text-ink-muted">
								{{ copy.text }}
							</p>
						</template>
					</div>
				</div>
			</div>
		</template>
	</div>
</template>

<script setup lang="ts">
	import RequiresAccount from '@/components/general/RequiresAccount.vue';
	import { buttonVariants } from '@/components/ui/button';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { gateMode } from '@/utils/localOnly';
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

	const mode = computed(() =>
		gateMode({
			entry: store.getters['featureToggles/moduleEntry'](props.featureKey),
			localOnly: store.getters.isLocalOnly === true,
			localWorkspace: store.getters.currentWorkspace?.is_local === true,
		}),
	);

	const copy = computed(() =>
		gateCopy(
			store.getters['featureToggles/canManageModules'],
			store.getters['featureToggles/isHiddenByMe'](props.featureKey),
		),
	);
</script>
