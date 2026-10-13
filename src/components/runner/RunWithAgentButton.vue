<template>
	<template v-if="model.visible">
		<Button
			v-if="model.mode === 'open'"
			variant="outline"
			size="sm"
			data-testid="agent-open-terminal"
			@click="openTerminal"
		>
			<SquareTerminal />
			Open terminal
		</Button>

		<Button
			v-else-if="model.disabled"
			variant="outline"
			size="sm"
			type="button"
			aria-disabled="true"
			:title="model.tooltip ?? undefined"
			class="cursor-not-allowed opacity-50"
			data-testid="agent-run-button"
		>
			<Bot />
			Run with agent
		</Button>

		<Popover v-else v-model:open="open">
			<PopoverTrigger as-child>
				<Button variant="outline" size="sm" data-testid="agent-run-button">
					<Bot />
					Run with agent
				</Button>
			</PopoverTrigger>
			<PopoverContent
				class="w-72"
				align="start"
				data-testid="agent-run-popover"
			>
				<form class="flex flex-col gap-3" @submit.prevent="run">
					<label class="flex flex-col gap-1 text-xs font-medium text-ink">
						Agent profile
						<select
							v-model="profile"
							class="h-9 rounded-md border border-input bg-background px-2 text-sm"
							data-testid="agent-profile"
						>
							<option v-for="item in profiles" :key="item.id" :value="item.id">
								{{ item.label }}
							</option>
						</select>
					</label>
					<p
						class="text-xs text-ink-subtle"
						data-testid="agent-permission-note"
					>
						Permission prompts: on
					</p>
					<p v-if="error" class="text-xs text-destructive" role="alert">
						{{ error }}
					</p>
					<Button type="submit" size="sm" :disabled="starting || !profile">
						{{ starting ? 'Starting…' : 'Run' }}
					</Button>
				</form>
			</PopoverContent>
		</Popover>
	</template>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import {
		Popover,
		PopoverContent,
		PopoverTrigger,
	} from '@/components/ui/popover';
	import {
		isRunnerSupported,
		runnerOpenTerminal,
		runnerProfiles,
		type RunnerProfile,
	} from '@/runner/api';
	import { runButtonModel, startRunForTask } from '@/runner/runButton';
	import { refreshRunnerStatus, useRunner } from '@/runner/store';
	import { Bot, SquareTerminal } from 'lucide-vue-next';
	import { computed, defineComponent, ref, watch, type PropType } from 'vue';

	type Id = number | string | null | undefined;

	export default defineComponent({
		name: 'RunWithAgentButton',
		components: {
			Bot,
			Button,
			Popover,
			PopoverContent,
			PopoverTrigger,
			SquareTerminal,
		},
		props: {
			workspaceId: { type: [Number, String] as PropType<Id>, default: null },
			taskId: { type: [Number, String] as PropType<Id>, default: null },
		},
		setup(props) {
			const runner = useRunner();
			const open = ref(false);
			const profiles = ref<RunnerProfile[]>([]);
			const profile = ref('');
			const starting = ref(false);
			const error = ref('');

			const workspaceId = computed(() =>
				props.workspaceId == null ? null : Number(props.workspaceId),
			);
			const session = computed(() =>
				workspaceId.value !== null && props.taskId != null
					? runner.sessionForTask(workspaceId.value, Number(props.taskId))
					: null,
			);
			const model = computed(() =>
				runButtonModel({
					supported: isRunnerSupported(),
					enabled: runner.enabled.value,
					status: runner.status.value,
					workspaceId: workspaceId.value,
					hasSession:
						session.value !== null && session.value.state.kind !== 'exited',
				}),
			);

			watch(open, async (value) => {
				if (!value) return;
				error.value = '';
				void refreshRunnerStatus();
				try {
					profiles.value = await runnerProfiles();
					if (!profiles.value.some((item) => item.id === profile.value))
						profile.value = profiles.value[0]?.id ?? '';
				} catch (failure) {
					error.value = String(failure);
				}
			});

			const run = async () => {
				if (
					workspaceId.value === null ||
					props.taskId == null ||
					!profile.value
				)
					return;
				starting.value = true;
				error.value = '';
				const result = await startRunForTask({
					workspaceId: workspaceId.value,
					taskId: Number(props.taskId),
					profile: profile.value,
				});
				starting.value = false;
				if (result.ok) open.value = false;
				else if (!result.cancelled) error.value = result.error;
			};

			const openTerminal = async () => {
				if (session.value) await runnerOpenTerminal(session.value.id);
			};

			return {
				model,
				open,
				profiles,
				profile,
				starting,
				error,
				run,
				openTerminal,
			};
		},
	});
</script>
