<script lang="ts">
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { LEGACY_MOVE_MESSAGE, type MoveView } from '@/local/moveToCloud/flow';
	import { stepIndex } from '@/local/moveToCloud/model';
	import { summarize } from '@/local/moveToCloud/progress';
	import { reportLines, serverErrorLines } from '@/local/moveToCloud/report';
	import { getMover } from '@/local/moveToCloud/runtime';
	import { openCloudWorkspace } from '@/utils/openCloudWorkspace';
	import {
		Circle,
		CircleAlert,
		CircleCheck,
		Loader,
		TriangleAlert,
	} from 'lucide-vue-next';
	import { computed, defineComponent, onBeforeUnmount, ref, watch } from 'vue';
	import { useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	const serverError = (view: MoveView): string =>
		serverErrorLines(view.status)[0] ?? 'The move failed on the server.';

	export default defineComponent({
		name: 'MoveToCloudProgress',
		components: {
			Button,
			Circle,
			CircleAlert,
			CircleCheck,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			Loader,
			TriangleAlert,
		},
		props: {
			open: { type: Boolean, default: false },
			code: { type: String, default: '' },
			name: { type: String, default: '' },
		},
		emits: ['update:open'],
		setup(props, { emit }) {
			const store = useStore();
			const router = useRouter();
			const view = ref<MoveView>(getMover().view(props.code));
			const opening = ref(false);
			const openError = ref('');
			let unsubscribe: (() => void) | null = null;

			const attach = () => {
				unsubscribe?.();
				view.value = getMover().view(props.code);
				unsubscribe = getMover().subscribe((next) => {
					if (next.code === props.code) view.value = next;
				});
			};
			watch(
				() => [props.open, props.code],
				() => props.open && attach(),
				{
					immediate: true,
				},
			);
			onBeforeUnmount(() => unsubscribe?.());

			const done = computed(() => !view.value.job && !!view.value.published);
			const summary = computed(() => summarize(view.value));
			const failure = computed(() => {
				if (view.value.legacy) return LEGACY_MOVE_MESSAGE;
				if (view.value.job?.error) return view.value.job.error;
				if (view.value.failedOnServer) return serverError(view.value);
				return '';
			});
			const differences = computed(() =>
				view.value.failedOnServer
					? [
							...serverErrorLines(view.value.status).slice(1),
							...reportLines(view.value.status?.report),
						]
					: [],
			);
			const cancellable = computed(
				() =>
					!!view.value.job &&
					stepIndex(view.value.job.step) < stepIndex('committed'),
			);
			const title = computed(() =>
				done.value
					? `Moved ${props.name} to the cloud`
					: `Moving ${props.name} to the cloud`,
			);
			const stopped = computed(() => view.value.job?.stoppedTimers ?? 0);

			const openWorkspace = async () => {
				const id = view.value.published?.workspaceId;
				if (id == null) return;
				opening.value = true;
				openError.value = '';
				try {
					await openCloudWorkspace(store, router, id);
					emit('update:open', false);
				} catch (e) {
					openError.value = (e as Error).message;
				} finally {
					opening.value = false;
				}
			};

			return {
				cancellable,
				differences,
				close: () => emit('update:open', false),
				done,
				failure,
				opening,
				openError,
				openWorkspace,
				stopped,
				summary,
				title,
				view,
				retry: () => void getMover().retry(props.code),
				discard: () => void getMover().discard(props.code),
				cancelMove: () => void getMover().cancel(props.code),
				publishAnyway: () => void getMover().publishAnyway(props.code),
			};
		},
	});
</script>

<template>
	<Dialog :open="open" @update:open="$emit('update:open', $event)">
		<DialogContent class="max-w-xl" data-testid="move-to-cloud-progress">
			<DialogHeader>
				<DialogTitle>{{ title }}</DialogTitle>
				<DialogDescription class="sr-only"
					>Progress of the move to your cloud account.</DialogDescription
				>
			</DialogHeader>

			<div v-if="done" class="flex flex-col items-start gap-3">
				<p class="text-sm">Done. Open the cloud workspace.</p>
				<Button :disabled="opening" @click="openWorkspace"
					>Open the cloud workspace</Button
				>
				<p v-if="openError" role="alert" class="text-sm text-destructive">
					{{ openError }}
				</p>
			</div>

			<template v-else>
				<div>
					<p class="mb-2 text-center text-sm">
						{{ summary.percent }}% - Step {{ summary.step }} of
						{{ summary.steps }}
					</p>
					<div
						class="h-2 w-full overflow-hidden rounded-full bg-muted"
						role="progressbar"
						:aria-valuenow="summary.percent"
						aria-valuemin="0"
						aria-valuemax="100"
					>
						<div
							class="h-full bg-destructive transition-all"
							:style="{ width: `${summary.percent}%` }"
						/>
					</div>
				</div>

				<ul class="flex flex-col gap-2 border-t pt-3 text-sm">
					<li
						v-for="row in summary.rows"
						:key="row.id"
						class="flex items-center gap-2"
					>
						<CircleCheck
							v-if="row.state === 'done'"
							class="size-4 text-emerald-500"
						/>
						<TriangleAlert
							v-else-if="row.state === 'warn'"
							class="size-4 text-amber-500"
						/>
						<Loader
							v-else-if="row.state === 'active'"
							class="size-4 animate-spin text-ink-subtle"
						/>
						<Circle v-else class="size-4 text-ink-subtle" />
						<span
							:class="{
								'text-amber-600 dark:text-amber-400': row.state === 'warn',
							}"
							>{{ row.label }}</span
						>
					</li>
				</ul>

				<div
					v-if="failure"
					role="alert"
					class="flex flex-col gap-2 rounded-md border border-destructive/40 p-3 text-sm"
				>
					<p class="flex gap-2 text-destructive">
						<CircleAlert class="mt-0.5 size-4 shrink-0" />
						<span class="break-words">{{ failure }}</span>
					</p>
					<ul
						v-if="differences.length"
						class="list-disc pl-9 text-xs text-ink-subtle"
					>
						<li v-for="line in differences" :key="line" class="break-words">
							{{ line }}
						</li>
					</ul>
					<div class="flex flex-wrap gap-2">
						<Button
							v-if="!view.legacy"
							size="sm"
							:disabled="view.busy"
							@click="retry"
							>Retry</Button
						>
						<Button
							v-if="view.canPublishAnyway"
							size="sm"
							variant="outline"
							:disabled="view.busy"
							@click="publishAnyway"
						>
							Publish anyway
						</Button>
						<Button
							size="sm"
							variant="outline"
							:disabled="view.busy"
							@click="discard"
							>Discard</Button
						>
					</div>
				</div>

				<p v-if="stopped > 0" class="text-xs text-ink-subtle">
					{{ stopped }} running
					{{ stopped === 1 ? 'timer was' : 'timers were' }} stopped.
				</p>
				<div class="border-t pt-3 text-xs text-ink-subtle">
					<p>
						You can close this window. The move continues and resumes if it
						stops.
					</p>
					<p>The workspace appears in your account when it is complete.</p>
				</div>
			</template>

			<DialogFooter>
				<Button
					v-if="cancellable && !failure"
					variant="ghost"
					@click="cancelMove"
					>Cancel move</Button
				>
				<Button v-if="!done" variant="outline" @click="close"
					>Run in background</Button
				>
				<Button variant="destructive" @click="close">Close</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>
