<template>
	<Dialog :open="true" @update:open="(open) => !open && onClose()">
		<DialogContent
			class="max-h-[calc(100dvh-2rem)] w-[calc(100%-1.5rem)] max-w-[460px] gap-3 overflow-y-auto border-line bg-surface p-4 text-ink sm:p-5"
			@interact-outside="(event) => locked && event.preventDefault()"
			@escape-key-down="(event) => locked && event.preventDefault()"
		>
			<DialogHeader>
				<DialogTitle class="text-base text-ink">{{ title }}</DialogTitle>
				<DialogDescription class="text-xs text-ink-subtle">
					{{ subtitle }}
				</DialogDescription>
			</DialogHeader>

			<template v-if="step === 'setup'">
				<label class="flex flex-col gap-1 text-xs font-medium text-ink-subtle">
					Cloud workspace
					<select
						v-model.number="workspaceId"
						class="w-full rounded-md border border-line bg-surface-sunken px-3 py-2 text-sm text-ink"
						@change="loadCategories"
					>
						<option v-for="w in targets" :key="w.id" :value="w.id">
							{{ w.name }}
						</option>
					</select>
				</label>
				<label
					v-if="mode === 'tasks'"
					class="flex flex-col gap-1 text-xs font-medium text-ink-subtle"
				>
					Category
					<select
						v-model="categoryId"
						class="w-full rounded-md border border-line bg-surface-sunken px-3 py-2 text-sm text-ink"
					>
						<option :value="null">No category</option>
						<option v-for="c in categories" :key="c.id" :value="c.id">
							{{ c.title }}
						</option>
					</select>
					<span v-if="categoriesError" class="font-normal text-destructive">
						{{ categoriesError }}
					</span>
				</label>
				<p v-if="mode === 'tasks'" class="text-xs text-ink-subtle">
					The server assigns the ticket key.
				</p>
				<div class="flex justify-end gap-2 pt-1">
					<button type="button" :class="ghostButton" @click="onClose">
						Cancel
					</button>
					<button
						type="button"
						:class="primaryButton"
						:disabled="!workspaceId"
						@click="step = 'confirm'"
					>
						Continue
					</button>
				</div>
			</template>

			<template v-else-if="step === 'confirm'">
				<p class="text-sm text-ink" data-testid="move-routines-statement">
					{{ routines.length }}
					{{ routines.length === 1 ? 'routine' : 'routines' }} will be sent to
					the TMGR server and will leave this device. Each local copy is removed
					after the server confirms it.
				</p>
				<ul
					class="divide-y divide-line max-h-40 overflow-y-auto rounded-md border border-line bg-surface-sunken text-sm"
				>
					<li
						v-for="r in routines"
						:key="r.id"
						class="flex items-center gap-2 px-3 py-1.5"
					>
						<span class="min-w-0 flex-1 truncate">{{ r.title }}</span>
						<span
							v-if="r.pattern"
							class="shrink-0 text-2xs tracking-wide text-ink-subtle uppercase"
							>Repeats</span
						>
					</li>
				</ul>
				<p class="text-xs text-ink-subtle">
					Destination: <strong class="text-ink">{{ targetName }}</strong
					>{{ mode === 'tasks' ? ` · ${categoryName}` : '' }}
				</p>
				<ul class="flex list-disc flex-col gap-1 pl-4 text-xs text-ink-subtle">
					<li v-if="hasHistory">
						Past completions are not moved; the cloud copy starts fresh.
					</li>
					<li v-if="mode === 'routines'">
						Title, description, schedule and repeat pattern are kept. Category
						and priority are not.
					</li>
					<li v-else>
						The tasks keep the title and description only. Schedule, repeat
						pattern and duration are not kept.
					</li>
				</ul>
				<p v-if="error" role="alert" class="text-xs text-destructive">
					{{ error }}
				</p>
				<div class="flex justify-end gap-2 pt-1">
					<button type="button" :class="ghostButton" @click="step = 'setup'">
						Back
					</button>
					<button
						type="button"
						:class="primaryButton"
						data-testid="move-routines-confirm"
						@click="start"
					>
						Send to TMGR and remove here
					</button>
				</div>
			</template>

			<p
				v-else-if="step === 'running'"
				class="py-6 text-center text-sm text-ink-subtle"
				role="status"
			>
				Sending to {{ targetName }}…
			</p>

			<template v-else>
				<p
					v-if="stopMessage"
					role="alert"
					class="rounded-md border border-line bg-surface-sunken px-3 py-2 text-xs text-destructive"
				>
					{{ stopMessage }}
				</p>
				<p class="text-sm text-ink" data-testid="move-routines-summary">
					{{ movedCount }} of {{ results.length }} moved to {{ targetName }}.
				</p>
				<ul
					class="divide-y divide-line max-h-56 overflow-y-auto rounded-md border border-line bg-surface-sunken text-sm"
				>
					<li
						v-for="item in results"
						:key="item.id"
						class="flex flex-col gap-1 px-3 py-2"
						:data-status="item.status"
					>
						<div class="flex items-center gap-2">
							<span class="min-w-0 flex-1 truncate">{{ item.title }}</span>
							<span
								class="shrink-0 text-2xs font-semibold tracking-wide uppercase"
								:class="
									item.status === 'moved'
										? 'text-status-done'
										: 'text-destructive'
								"
								>{{ statusLabel[item.status] }}</span
							>
						</div>
						<p
							v-if="item.message && item.status !== 'moved'"
							class="text-xs text-ink-subtle"
						>
							{{ item.message }}
						</p>
						<button
							v-if="item.status === 'in_cloud'"
							type="button"
							class="self-start text-xs font-medium text-brand hover:underline"
							:disabled="busy"
							@click="retry([item.id])"
						>
							Remove local copy
						</button>
					</li>
				</ul>
				<div class="flex flex-wrap justify-end gap-2 pt-1">
					<button
						v-if="retryable.length"
						type="button"
						:class="ghostButton"
						:disabled="busy"
						data-testid="move-routines-retry"
						@click="retry(retryable)"
					>
						Retry {{ retryable.length }} failed
					</button>
					<button
						v-if="movedCount"
						type="button"
						:class="ghostButton"
						:disabled="busy"
						@click="openTarget"
					>
						Open {{ targetName }}
					</button>
					<button
						type="button"
						:class="primaryButton"
						:disabled="busy"
						data-testid="move-routines-done"
						@click="onClose"
					>
						Done
					</button>
				</div>
			</template>
		</DialogContent>
	</Dialog>
</template>

<script setup lang="ts">
	import { deleteDailyTask } from '@/actions/tmgr/daily-tasks';
	import type { Workspace } from '@/actions/tmgr/workspaces';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { activeLocalWorkspaceCode } from '@/local/runtime';
	import { openCloudWorkspace } from '@/utils/openCloudWorkspace';
	import { computed, ref, type PropType } from 'vue';
	import { useRouter } from 'vue-router';
	import { useStore } from 'vuex';
	import {
		retryMove,
		runMove,
		STOP_MESSAGE,
		toMessage,
		type ItemResult,
		type ItemStatus,
		type MoveMode,
		type MoveOutcome,
		type MoveTarget,
	} from './batch';
	import { type CloudApi, type CloudCategory } from './cloud';
	import { serverCloudApi } from './cloudHttp';
	import { createMoveDeps, prepareTarget } from './moveDeps';
	import type { LocalRoutine } from './payload';

	const props = defineProps({
		routines: { type: Array as PropType<LocalRoutine[]>, required: true },
		mode: { type: String as PropType<MoveMode>, required: true },
		targets: { type: Array as PropType<Workspace[]>, required: true },
	});
	const emit = defineEmits<{ (e: 'close', moved: boolean): void }>();

	const store = useStore();
	const router = useRouter();

	const ghostButton =
		'rounded-md border border-line px-3 py-2 text-sm font-medium text-ink transition-colors hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-50';
	const primaryButton =
		'rounded-md bg-brand px-3 py-2 text-sm font-semibold text-on-accent transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:bg-surface-sunken disabled:text-ink-subtle';

	const step = ref<'setup' | 'confirm' | 'running' | 'result'>('setup');
	const workspaceId = ref<number | null>(props.targets[0]?.id ?? null);
	const categoryId = ref<number | null>(null);
	const categories = ref<CloudCategory[]>([]);
	const categoriesError = ref('');
	const error = ref('');
	const results = ref<ItemResult[]>([]);
	const stopped = ref<MoveOutcome['stopped']>(null);
	const busy = ref(false);
	const locked = computed(() => step.value === 'running' || busy.value);
	const localCode = activeLocalWorkspaceCode();
	let api: CloudApi | null = null;
	let target: MoveTarget | null = null;

	const statusLabel: Record<ItemStatus, string> = {
		moved: 'Moved',
		failed: 'Failed',
		in_cloud: 'In the cloud, local copy not removed',
		not_sent: 'Not sent',
	};

	const title = computed(() =>
		props.mode === 'routines'
			? 'Move to cloud routines'
			: 'Convert to cloud tasks',
	);
	const subtitle = computed(() =>
		step.value === 'setup'
			? 'Choose where the selected local routines go.'
			: `${props.routines.length} selected`,
	);
	const targetName = computed(
		() => props.targets.find((w) => w.id === workspaceId.value)?.name ?? '',
	);
	const categoryName = computed(
		() =>
			categories.value.find((c) => c.id === categoryId.value)?.title ??
			'No category',
	);
	const hasHistory = computed(() =>
		props.routines.some((r) => r.pattern || r.hasHistory),
	);
	const movedCount = computed(
		() => results.value.filter((r) => r.status === 'moved').length,
	);
	const retryable = computed(() =>
		results.value
			.filter((r) => r.status === 'failed' || r.status === 'not_sent')
			.map((r) => r.id),
	);
	const stopMessage = computed(() =>
		stopped.value ? STOP_MESSAGE[stopped.value] : '',
	);

	const getApi = async () => (api ??= await serverCloudApi());

	const loadCategories = async () => {
		categoryId.value = null;
		categories.value = [];
		categoriesError.value = '';
		const requested = workspaceId.value;
		if (props.mode !== 'tasks' || !requested) return;
		try {
			const list = await (await getApi()).categories(requested);
			if (requested === workspaceId.value) categories.value = list;
		} catch (e) {
			categoriesError.value = `Categories could not be loaded: ${toMessage(e)}`;
		}
	};
	void loadCategories();

	const removeLocal = async (id: number) => {
		if (activeLocalWorkspaceCode() !== localCode)
			throw new Error(
				'The open local workspace changed, so the local copy was not removed.',
			);
		try {
			await deleteDailyTask(id);
		} catch (e) {
			if ((e as any)?.response?.status !== 404) throw e;
		}
	};

	const settle = (outcome: MoveOutcome) => {
		results.value = outcome.results;
		stopped.value = outcome.stopped;
		step.value = 'result';
	};

	const run = async (
		action: (
			target: MoveTarget,
			deps: ReturnType<typeof createMoveDeps>,
		) => Promise<MoveOutcome>,
	) => {
		let cloud: CloudApi;
		try {
			cloud = await getApi();
			target ??= await prepareTarget(
				cloud,
				props.mode,
				workspaceId.value as number,
				categoryId.value,
			);
		} catch (e) {
			error.value = toMessage(e);
			step.value = 'confirm';
			return;
		}
		settle(await action(target, createMoveDeps(cloud, removeLocal)));
	};

	const start = async () => {
		error.value = '';
		step.value = 'running';
		await run((t, deps) => runMove(props.routines, props.mode, t, deps));
	};

	const retry = async (ids: number[]) => {
		busy.value = true;
		try {
			await run((t, deps) =>
				retryMove(props.routines, results.value, ids, props.mode, t, deps),
			);
		} finally {
			busy.value = false;
		}
	};

	const openTarget = async () => {
		const id = workspaceId.value as number;
		emit('close', movedCount.value > 0);
		await openCloudWorkspace(store as any, router, id);
		if (props.mode === 'routines') await router.push('/routines');
	};

	const onClose = () => {
		if (locked.value) return;
		emit(
			'close',
			results.value.some((r) => r.status === 'moved'),
		);
	};
</script>
