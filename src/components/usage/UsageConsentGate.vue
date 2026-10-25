<template>
	<SettingsSection
		v-if="gate === 'ask'"
		title="Read AI tool logs?"
		data-testid="usage-consent"
	>
		<div class="flex flex-col gap-4 text-sm">
			<p class="text-ink-subtle">
				To show cost and limits, TMGR reads the session logs Claude Code and
				Codex write on this computer. Pick which ones:
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
							>No logs found on this computer</span
						>
					</span>
				</label>
			</div>
			<p class="text-ink-subtle">
				<span class="font-medium text-ink">Kept on this device:</span> token
				counts, cost, model names, timestamps, session ids, project folders and
				git branches, and the names of tools, skills and commands the AI used,
				file types it edited and pull requests it opened. What you or the AI
				wrote is never stored.
			</p>
			<p class="text-ink-subtle">
				Nothing leaves this device unless you set up uploads with
				<CommandChip command="tmgr usage sync" />.
			</p>
			<p class="text-ink-subtle" data-testid="usage-consent-scope">
				Your choice covers AI usage in this app, background jobs, agent
				notifications and the tmgr CLI on this device. This setting doesn't
				cover the TMGR runner yet: for sessions it starts, it reads their logs
				for titles, questions and status, saves detected questions with your
				prompt and the reply on this device, and may send reply text to Claude
				(Haiku) to spot questions.
			</p>
			<p v-if="current.has_data" class="text-ink-subtle">
				Numbers TMGR read earlier stay on this device. To delete them, use
				Forget scanned data in
				<router-link
					to="/settings?tab=privacy&section=ai-usage"
					class="text-primary hover:underline"
					>Settings → Privacy</router-link
				>.
			</p>
			<p v-if="error" role="alert" class="text-status-fix-fg">{{ error }}</p>
			<div class="grid grid-cols-2 gap-3 sm:max-w-sm">
				<Button
					variant="outline"
					:disabled="busy"
					@click="choose({ state: 'denied' })"
				>
					Not now
				</Button>
				<Button
					variant="outline"
					:disabled="busy || (!picked.claude && !picked.codex)"
					@click="choose({ state: 'granted', ...picked })"
				>
					Allow
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
			<p class="font-medium text-ink">Reading AI tool logs is off</p>
			<p class="mt-0.5 text-ink-subtle">
				TMGR doesn't read your Claude Code or Codex logs for AI usage on this
				device. Change it here or in
				<router-link
					to="/settings?tab=privacy&section=ai-usage"
					class="text-primary hover:underline"
					>Settings → Privacy</router-link
				>.
			</p>
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
			Allow
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
