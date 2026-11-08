<template>
	<section
		class="rounded-card border border-line bg-surface-sunken p-4 text-sm text-ink"
		aria-labelledby="still-waiting-title"
		data-testid="still-waiting"
	>
		<h2 id="still-waiting-title" class="text-base font-semibold">
			{{ $t('onboarding.stillWaiting.title') }}
		</h2>
		<p class="mt-1 text-ink-subtle">
			{{ $t('onboarding.stillWaiting.intro') }}
		</p>
		<ol class="space-y-2 mt-3 list-decimal pl-5">
			<li>{{ $t('onboarding.stillWaiting.runCommand', { client }) }}</li>
			<li>{{ $t('onboarding.stillWaiting.restart') }}</li>
			<i18n-t
				v-if="verifyCommand"
				keypath="onboarding.stillWaiting.verify"
				tag="li"
			>
				<template #command><CommandChip :command="verifyCommand" /></template>
			</i18n-t>
		</ol>
		<div class="mt-4 flex flex-wrap gap-2">
			<button
				type="button"
				class="min-h-[44px] rounded-md bg-brand px-4 text-sm font-medium text-on-accent hover:opacity-90 sm:min-h-[36px]"
				data-tour-avoid="cta"
				@click="$emit('check-again')"
			>
				{{ $t('onboarding.stillWaiting.checkAgain') }}
			</button>
			<button
				type="button"
				class="min-h-[44px] rounded-md border border-line px-4 text-sm font-medium text-ink hover:bg-surface sm:min-h-[36px]"
				@click="$emit('different-client')"
			>
				{{ $t('onboarding.stillWaiting.differentClient') }}
			</button>
		</div>
		<div class="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1">
			<router-link
				:to="{ name: 'AgentsGuide' }"
				class="inline-flex min-h-[44px] items-center text-ink-subtle underline hover:text-ink sm:min-h-0"
			>
				{{ $t('onboarding.stillWaiting.guide') }}
			</router-link>
			<button
				type="button"
				class="inline-flex min-h-[44px] items-center text-ink-subtle underline hover:text-ink sm:min-h-0"
				data-tour-avoid="cta"
				@click="$emit('later')"
			>
				{{ $t('onboarding.stillWaiting.later') }}
			</button>
		</div>
	</section>
</template>

<script lang="ts">
	import CommandChip from '@/components/general/CommandChip.vue';
	import { t } from '@/i18n';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'StillWaiting',
		components: { CommandChip },
		props: {
			client: {
				type: String,
				default: () => t('onboarding.stillWaiting.defaultClient'),
			},
			verifyCommand: { type: String, default: '' },
		},
		emits: ['check-again', 'different-client', 'later'],
	});
</script>
