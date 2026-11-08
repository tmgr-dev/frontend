<template>
	<SettingsSection
		:title="$t('onboarding.tours.tips.title')"
		:description="$t('onboarding.tours.tips.description')"
		data-tour="settings.tips"
	>
		<div class="flex flex-col gap-4">
			<fieldset :key="renderKey" class="flex flex-col gap-2" :disabled="busy">
				<legend class="sr-only">
					{{ $t('onboarding.tours.tips.legend') }}
				</legend>
				<label
					v-for="option in options"
					:key="option.value"
					class="flex cursor-pointer items-start gap-2.5 text-sm"
				>
					<input
						type="radio"
						name="tips-mode"
						class="mt-1 h-4 w-4 accent-brand"
						:value="option.value"
						:checked="mode === option.value"
						@change="choose(option.value)"
					/>
					<span>
						<span class="block font-medium text-ink">{{ option.label }}</span>
						<span class="block text-ink-subtle">{{ option.description }}</span>
					</span>
				</label>
			</fieldset>

			<p v-if="failed" role="alert" class="text-sm text-status-fix-fg">
				{{ $t('onboarding.tours.tips.saveFailed') }}
			</p>

			<div class="flex flex-wrap items-center gap-3">
				<template v-if="!confirming">
					<Button
						variant="outline"
						size="sm"
						data-testid="tips-reset"
						:disabled="busy"
						@click="confirming = true"
					>
						{{ $t('onboarding.tours.tips.resetAll') }}
					</Button>
				</template>
				<template v-else>
					<span class="text-sm text-ink">{{
						$t('onboarding.tours.tips.resetPrompt')
					}}</span>
					<Button
						variant="destructive"
						size="sm"
						data-testid="tips-reset-confirm"
						:disabled="busy"
						@click="reset"
					>
						{{ $t('common.actions.reset') }}
					</Button>
					<Button variant="ghost" size="sm" @click="confirming = false">
						{{ $t('common.actions.cancel') }}
					</Button>
				</template>
			</div>
		</div>
	</SettingsSection>
</template>

<script lang="ts">
	import { t } from '@/i18n';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import type { TipsMode } from '@/tours/types';
	import { useTours } from '@/tours/useTours';
	import { computed, defineComponent, ref } from 'vue';

	const OPTIONS: { value: TipsMode; path: string }[] = [
		{ value: 'auto', path: 'onboarding.tours.tips.options.auto' },
		{ value: 'hub_only', path: 'onboarding.tours.tips.options.hubOnly' },
		{ value: 'off', path: 'onboarding.tours.tips.options.off' },
	];

	export default defineComponent({
		name: 'TipsSettings',
		components: { SettingsSection, Button },
		setup() {
			const tours = useTours();
			const busy = ref(false);
			const confirming = ref(false);
			const failed = ref(false);
			const renderKey = ref(0);
			const mode = computed<TipsMode>(
				() => tours.onboarding.value?.tips_mode ?? 'auto',
			);

			const guarded = async (work: () => Promise<void>) => {
				busy.value = true;
				failed.value = false;
				try {
					await work();
				} catch (error) {
					console.error(error);
					failed.value = true;
					renderKey.value += 1;
				} finally {
					busy.value = false;
				}
			};

			return {
				options: computed(() =>
					OPTIONS.map((option) => ({
						value: option.value,
						label: t(`${option.path}.label`),
						description: t(`${option.path}.description`),
					})),
				),
				mode,
				busy,
				failed,
				renderKey,
				confirming,
				choose: (value: TipsMode) => guarded(() => tours.setTipsMode(value)),
				reset: () =>
					guarded(async () => {
						await tours.resetAll();
						confirming.value = false;
					}),
			};
		},
	});
</script>
