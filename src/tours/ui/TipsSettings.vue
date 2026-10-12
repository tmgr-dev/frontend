<template>
	<SettingsSection
		title="Tips and tours"
		description="Short guided tours that show up as you use TMGR."
		data-tour="settings.tips"
	>
		<div class="flex flex-col gap-4">
			<fieldset class="flex flex-col gap-2" :disabled="busy">
				<legend class="sr-only">When to show tips</legend>
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

			<div class="flex flex-wrap items-center gap-3">
				<template v-if="!confirming">
					<Button
						variant="outline"
						size="sm"
						data-testid="tips-reset"
						:disabled="busy"
						@click="confirming = true"
					>
						Reset all tours
					</Button>
				</template>
				<template v-else>
					<span class="text-sm text-ink">Start every tour over?</span>
					<Button
						variant="destructive"
						size="sm"
						data-testid="tips-reset-confirm"
						:disabled="busy"
						@click="reset"
					>
						Reset
					</Button>
					<Button variant="ghost" size="sm" @click="confirming = false">
						Cancel
					</Button>
				</template>
			</div>
		</div>
	</SettingsSection>
</template>

<script lang="ts">
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import type { TipsMode } from '@/tours/types';
	import { useTours } from '@/tours/useTours';
	import { computed, defineComponent, ref } from 'vue';

	const OPTIONS: { value: TipsMode; label: string; description: string }[] = [
		{
			value: 'all',
			label: 'Show tips automatically',
			description: 'A short tour is offered the first time you open a feature.',
		},
		{
			value: 'minimal',
			label: 'Only from Getting started',
			description: 'No automatic tours. Start any tour yourself from the Getting started page.',
		},
		{
			value: 'off',
			label: 'Never show tips',
			description: 'Tours stay on the Getting started page if you want them.',
		},
	];

	export default defineComponent({
		name: 'TipsSettings',
		components: { SettingsSection, Button },
		setup() {
			const tours = useTours();
			const busy = ref(false);
			const confirming = ref(false);
			const mode = computed<TipsMode>(
				() => tours.onboarding.value?.tips_mode ?? 'all',
			);

			const guarded = async (work: () => Promise<void>) => {
				busy.value = true;
				try {
					await work();
				} catch (error) {
					console.error(error);
				} finally {
					busy.value = false;
				}
			};

			return {
				options: OPTIONS,
				mode,
				busy,
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
