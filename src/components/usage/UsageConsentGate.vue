<template>
	<SettingsSection
		v-if="gate === 'ask'"
		:title="$t('agents.usage.consent.title')"
		data-testid="usage-consent"
	>
		<div class="flex flex-col gap-4 text-sm">
			<p class="text-ink-subtle">
				{{ $t('agents.usage.consent.intro') }}
			</p>
			<div class="flex flex-col gap-3">
				<label
					v-for="tool in TOOLS"
					:key="tool"
					class="flex cursor-pointer items-start gap-2"
				>
					<input
						v-model="picked[tool]"
						type="checkbox"
						class="mt-0.5 h-4 w-4 rounded accent-brand"
						:data-testid="`usage-consent-${tool}`"
					/>
					<span class="min-w-0">
						<span class="font-medium text-ink">{{ toolName(tool) }}</span>
						<span
							v-for="f in foldersOf(tool)"
							:key="f.path"
							class="block font-mono text-xs break-all text-ink-subtle"
							>{{ f.path }}</span
						>
						<span
							v-if="!foldersOf(tool).length"
							class="block text-xs text-ink-subtle"
							>{{ $t('agents.usage.noLogs') }}</span
						>
					</span>
				</label>
			</div>
			<p class="text-ink-subtle">
				<span class="font-medium text-ink">{{
					$t('agents.usage.consent.keptLabel')
				}}</span>
				{{ $t('agents.usage.consent.keptBody') }}
			</p>
			<i18n-t
				keypath="agents.usage.consent.uploads"
				tag="p"
				class="text-ink-subtle"
			>
				<template #command>
					<CommandChip command="tmgr usage sync" />
				</template>
			</i18n-t>
			<p class="text-ink-subtle" data-testid="usage-consent-scope">
				{{ $t('agents.usage.consent.scope') }}
			</p>
			<i18n-t
				v-if="current.has_data"
				keypath="agents.usage.consent.hasData"
				tag="p"
				class="text-ink-subtle"
			>
				<template #link>
					<router-link
						to="/settings?tab=privacy&section=ai-usage"
						class="text-primary hover:underline"
						>{{ $t('agents.usage.consent.settingsPrivacy') }}</router-link
					>
				</template>
			</i18n-t>
			<p v-if="error" role="alert" class="text-status-fix-fg">{{ error }}</p>
			<div class="grid grid-cols-2 gap-3 sm:max-w-sm">
				<Button
					variant="outline"
					:disabled="busy"
					@click="choose({ state: 'denied' })"
				>
					{{ $t('agents.usage.consent.notNow') }}
				</Button>
				<Button
					variant="outline"
					:disabled="busy || (!picked.claude && !picked.codex)"
					@click="choose({ state: 'granted', ...picked })"
				>
					{{ $t('agents.usage.consent.allow') }}
				</Button>
			</div>
		</div>
	</SettingsSection>

	<div
		v-else-if="gate === 'off'"
		data-testid="usage-consent-off"
		class="flex flex-col gap-3 rounded-lg border border-border bg-card p-4 sm:flex-row sm:items-center sm:justify-between"
	>
		<div class="min-w-0 text-sm">
			<p class="font-medium text-ink">
				{{ $t('agents.usage.consent.offTitle') }}
			</p>
			<i18n-t
				keypath="agents.usage.consent.offBody"
				tag="p"
				class="mt-0.5 text-ink-subtle"
			>
				<template #link>
					<router-link
						to="/settings?tab=privacy&section=ai-usage"
						class="text-primary hover:underline"
						>{{ $t('agents.usage.consent.settingsPrivacy') }}</router-link
					>
				</template>
			</i18n-t>
			<p v-if="error" role="alert" class="mt-1 text-status-fix-fg">
				{{ error }}
			</p>
		</div>
		<Button
			variant="outline"
			size="sm"
			:disabled="busy"
			@click="choose({ state: 'granted' })"
		>
			{{ $t('agents.usage.consent.allow') }}
		</Button>
	</div>
</template>

<script lang="ts">
	import CommandChip from '@/components/general/CommandChip.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import type { UsageConsent, UsageConsentChange } from '@/usage/api';
	import { toolName } from '@/usage/format';
	import {
		computed,
		defineComponent,
		reactive,
		watch,
		type PropType,
	} from 'vue';
	import { useUsageConsent } from './consent';

	const TOOLS = ['claude', 'codex'] as const;

	export default defineComponent({
		name: 'UsageConsentGate',
		components: { Button, CommandChip, SettingsSection },
		props: {
			consent: { type: Object as PropType<UsageConsent>, required: true },
		},
		emits: ['update'],
		setup(props, { emit }) {
			const { consent, busy, error, gate, save } = useUsageConsent(
				props.consent,
			);
			watch(
				() => props.consent,
				(c) => (consent.value = c),
			);
			const current = computed(() => consent.value ?? props.consent);
			const picked = reactive({
				claude: props.consent.claude,
				codex: props.consent.codex,
			});
			const foldersOf = (tool: string) =>
				current.value.folders.filter((f) => f.tool === tool);
			const choose = async (change: UsageConsentChange) => {
				if (await save(change)) emit('update', consent.value);
			};
			return {
				TOOLS,
				busy,
				error,
				gate,
				current,
				picked,
				foldersOf,
				choose,
				toolName,
			};
		},
	});
</script>
