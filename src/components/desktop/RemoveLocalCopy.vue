<template>
	<div class="flex flex-col gap-3" data-testid="remove-local-copy">
		<p class="text-sm text-ink-subtle">
			Copied to your cloud workspace <strong>{{ published.name }}</strong> on
			{{ movedOn }}. Removing the local copy deletes it from this computer; the
			archive in the backups folder stays.
		</p>

		<ul class="flex flex-col gap-1.5 text-sm">
			<li v-for="guard in guards" :key="guard.id" class="flex gap-2">
				<CircleCheck
					v-if="guard.ok"
					class="mt-0.5 size-4 shrink-0 text-emerald-500"
				/>
				<Circle v-else class="mt-0.5 size-4 shrink-0 text-ink-subtle" />
				<span>{{ guard.label }}</span>
			</li>
		</ul>

		<ul v-if="published.excluded.length" class="flex flex-col gap-2 text-sm">
			<li
				v-for="file in published.excluded"
				:key="file.id"
				class="flex flex-wrap items-center gap-2 rounded-md border p-2"
			>
				<span class="min-w-0 flex-1 break-all">
					{{ file.name }}
					<span class="text-xs text-ink-subtle"
						>({{ reasonLabel(file.reason) }})</span
					>
				</span>
				<Button
					v-if="file.reason !== 'missing'"
					size="sm"
					variant="outline"
					:disabled="busy"
					@click="exportFile(file)"
				>
					{{ file.resolution === 'exported' ? 'Exported' : 'Export' }}
				</Button>
				<label class="flex items-center gap-1.5 text-xs">
					<input
						type="checkbox"
						:checked="file.resolution === 'lost'"
						@change="
							acceptLost(file, ($event.target as HTMLInputElement).checked)
						"
					/>
					Accept as lost
				</label>
			</li>
		</ul>

		<div class="flex flex-wrap gap-2">
			<Button
				size="sm"
				variant="outline"
				:disabled="busy"
				@click="createArchive"
			>
				{{ published.archive ? 'Create a new archive' : 'Create archive' }}
			</Button>
			<Button
				size="sm"
				variant="destructive"
				:disabled="!allowed || busy"
				@click="confirmOpen = true"
			>
				Remove local copy…
			</Button>
		</div>
		<p v-if="error" role="alert" class="break-words text-sm text-destructive">
			{{ error }}
		</p>

		<ConfirmByNameDialog
			v-model:open="confirmOpen"
			title="Remove local copy"
			:description="`This deletes the local workspace ${workspace.name} and its files from this computer. The cloud copy and the archive in the backups folder stay. This cannot be undone.`"
			:expected-name="workspace.name"
			confirm-label="Remove local copy"
			:busy="busy"
			@confirm="remove"
		/>
	</div>
</template>

<script lang="ts">
	import ConfirmByNameDialog from '@/components/general/ConfirmByNameDialog.vue';
	import { Button } from '@/components/ui/button';
	import { useToast } from '@/components/ui/toast';
	import { formatMoveDate } from '@/local/moveToCloud/banner';
	import {
		recordArchive,
		removeLocalCopy,
		removeLocalCopyGuards,
		resolveExcludedFile,
	} from '@/local/moveToCloud/guards';
	import {
		browserStorage,
		loadPersisted,
		reasonLabel,
		type ExcludedFile,
		type PublishedMove,
	} from '@/local/moveToCloud/model';
	import { moveArchive, sourceRevision } from '@/local/moveToCloud/native';
	import { exportLocalFile, removeDeps } from '@/local/moveToCloud/runtime';
	import {
		activeLocalWorkspace,
		listLocalWorkspaces,
		setActiveLocalWorkspace,
	} from '@/local/runtime';
	import type { LocalWorkspace } from '@/local/types';
	import { openCloudWorkspace } from '@/utils/openCloudWorkspace';
	import { Circle, CircleCheck } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		onMounted,
		ref,
		type PropType,
	} from 'vue';
	import { useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	const message = (e: unknown) =>
		(e as any)?.message || String(e) || 'Something went wrong.';

	export default defineComponent({
		name: 'RemoveLocalCopy',
		components: { Button, CircleCheck, Circle, ConfirmByNameDialog },
		props: {
			workspace: { type: Object as PropType<LocalWorkspace>, required: true },
		},
		emits: ['removed'],
		setup(props, { emit }) {
			const store = useStore();
			const router = useRouter();
			const { toast } = useToast();
			const published = ref<PublishedMove>(
				loadPersisted(browserStorage, props.workspace.code).published!,
			);
			const liveRevision = ref<string | null>(null);
			const busy = ref(false);
			const error = ref('');
			const confirmOpen = ref(false);

			const reload = () => {
				const next = loadPersisted(
					browserStorage,
					props.workspace.code,
				).published;
				if (next) published.value = next;
			};
			const refreshRevision = async () => {
				try {
					liveRevision.value = await sourceRevision(props.workspace.code);
				} catch (e) {
					liveRevision.value = null;
					error.value = message(e);
				}
			};
			onMounted(refreshRevision);

			const guards = computed(() =>
				removeLocalCopyGuards({
					published: published.value,
					liveRevision: liveRevision.value,
				}),
			);

			const run = async (action: () => Promise<unknown>) => {
				busy.value = true;
				error.value = '';
				try {
					await action();
				} catch (e) {
					error.value = message(e);
					toast({ title: error.value, variant: 'destructive' });
				} finally {
					busy.value = false;
				}
			};

			const createArchive = () =>
				run(async () => {
					const archive = await moveArchive(props.workspace.code);
					recordArchive(browserStorage, props.workspace.code, archive);
					reload();
					await refreshRevision();
				});

			const exportFile = (file: ExcludedFile) =>
				run(async () => {
					await exportLocalFile(props.workspace.code, file);
					resolveExcludedFile(
						browserStorage,
						props.workspace.code,
						file.id,
						'exported',
					);
					reload();
				});

			const acceptLost = (file: ExcludedFile, checked: boolean) => {
				resolveExcludedFile(
					browserStorage,
					props.workspace.code,
					file.id,
					checked ? 'lost' : null,
				);
				reload();
			};

			const remove = (typedName: string) =>
				run(async () => {
					const wasActive =
						activeLocalWorkspace()?.code === props.workspace.code;
					const cloudId = published.value.workspaceId;
					await removeLocalCopy(removeDeps, props.workspace.code, typedName);
					confirmOpen.value = false;
					if (wasActive) {
						setActiveLocalWorkspace(null);
						if (cloudId != null)
							await openCloudWorkspace(store, router, cloudId);
					}
					await listLocalWorkspaces(true);
					await store.dispatch('loadWorkspaces');
					emit('removed');
				});

			return {
				acceptLost,
				allowed: computed(() => guards.value.every((g) => g.ok)),
				busy,
				confirmOpen,
				createArchive,
				error,
				exportFile,
				guards,
				movedOn: computed(() => formatMoveDate(published.value.at)),
				published,
				reasonLabel,
				remove,
			};
		},
	});
</script>
