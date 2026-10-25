<template>
	<div
		v-if="consent?.supported || error"
		ref="root"
		data-testid="ai-usage-settings"
	>
		<SettingsSection
			title="AI usage"
			description="The AI Usage page reads the session logs Claude Code and Codex write on this computer to show cost and limits. What you or the AI wrote is never stored."
		>
			<div class="flex flex-col gap-2">
				<template v-if="consent">
					<SettingsRow
						v-slot="{ labelId, descriptionId }"
						label="Read AI tool logs"
						:description="readingNote(consent.state)"
					>
						<Switch
							data-testid="usage-read-switch"
							:checked="reading"
							:disabled="busy"
							:aria-labelledby="labelId"
							:aria-describedby="descriptionId"
							@update:checked="
								(value) => save({ state: value ? 'granted' : 'denied' })
							"
						/>
					</SettingsRow>
					<SettingsRow
						v-for="tool in TOOLS"
						:key="tool"
						:label="toolName(tool)"
					>
						<template #description>
							<span
								v-for="f in foldersOf(tool)"
								:key="f.path"
								class="block font-mono text-xs break-all"
								>{{ f.path }}</span
							>
							<span v-if="!foldersOf(tool).length"
								>No logs found on this computer</span
							>
						</template>
						<template #default="{ labelId, descriptionId }">
							<Switch
								:data-testid="`usage-${tool}-switch`"
								:checked="reading && consent[tool]"
								:disabled="busy || !reading"
								:aria-labelledby="labelId"
								:aria-describedby="descriptionId"
								@update:checked="(value) => save({ [tool]: value })"
							/>
						</template>
					</SettingsRow>
					<SettingsRow
						label="Scanned data"
						description="Usage numbers TMGR keeps on this device."
					>
						<Button
							variant="outline"
							size="sm"
							:disabled="busy"
							:aria-busy="busy"
							data-testid="usage-forget"
							@click="confirmForget = true"
						>
							Forget scanned data
						</Button>
					</SettingsRow>
				</template>
				<p
					v-if="message"
					role="status"
					data-testid="usage-forget-result"
					class="text-sm text-ink-subtle"
				>
					{{ message }}
				</p>
				<p
					v-if="error"
					role="alert"
					data-testid="usage-settings-error"
					class="text-sm text-status-fix-fg"
				>
					{{ error }}
				</p>
				<div v-if="!consent">
					<Button variant="outline" size="sm" :disabled="busy" @click="load">
						Retry
					</Button>
				</div>
			</div>
		</SettingsSection>

		<AlertDialog v-model:open="confirmForget">
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>Forget scanned data?</AlertDialogTitle>
					<AlertDialogDescription>
						This deletes the usage numbers TMGR keeps on this device and turns
						reading off. Uploads set up with
						<code class="font-mono">tmgr usage sync</code> are turned off too;
						what was already uploaded is not deleted. Your AI tools' own logs
						are not touched. If the Claude Code status line hook is installed,
						it keeps saving limit snapshots here; they aren't read while reading
						is off.
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>Cancel</AlertDialogCancel>
					<AlertDialogAction
						class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
						@click="forget"
						>Forget</AlertDialogAction
					>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	</div>
</template>

<script lang="ts">
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import { Switch } from '@/components/ui/switch';
	import { toolName } from '@/usage/format';
	import { computed, defineComponent, nextTick, onMounted, ref } from 'vue';
	import { useRoute } from 'vue-router';
	import { readingNote, useUsageConsent } from './consent';

	const TOOLS = ['claude', 'codex'] as const;

	export default defineComponent({
		name: 'AiUsageSettings',
		components: {
			AlertDialog,
			AlertDialogAction,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			Button,
			SettingsRow,
			SettingsSection,
			Switch,
		},
		setup() {
			const { consent, busy, error, message, load, save, forget } =
				useUsageConsent();
			const confirmForget = ref(false);
			const reading = computed(() => consent.value?.state === 'granted');
			const foldersOf = (tool: string) =>
				consent.value?.folders.filter((f) => f.tool === tool) ?? [];
			const route = useRoute();
			const root = ref<HTMLElement | null>(null);
			onMounted(async () => {
				await load();
				if (route.query.section !== 'ai-usage') return;
				await nextTick();
				root.value?.scrollIntoView({ block: 'start' });
			});
			return {
				TOOLS,
				root,
				consent,
				busy,
				error,
				message,
				reading,
				confirmForget,
				foldersOf,
				load,
				save,
				forget,
				toolName,
				readingNote,
			};
		},
	});
</script>
