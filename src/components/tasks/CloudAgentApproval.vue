<template>
	<section
		v-if="visible"
		class="flex flex-col gap-2"
		data-testid="cloud-agent-approval"
	>
		<div class="flex flex-wrap items-center gap-2">
			<span
				class="text-2xs font-bold tracking-wide text-ink-subtle uppercase"
				>{{ $t('taskAgent.cloudApproval.agentRuns') }}</span
			>
			<span
				class="rounded-full px-2 py-0.5 text-xs"
				:class="chipClass"
				data-testid="approval-chip"
				>{{ chipText }}</span
			>
			<span class="flex-1" />
			<Button
				v-if="approval && needsConfirm"
				variant="outline"
				size="sm"
				data-testid="cloud-confirm-open"
				@click="openDialog"
			>
				{{ $t('taskAgent.cloudApproval.confirmForAgentRuns') }}
			</Button>
		</div>
		<p
			v-if="coverageHint"
			class="text-xs text-ink-subtle"
			data-testid="approval-coverage-hint"
		>
			{{ coverageHint }}
		</p>
		<p
			v-if="tooLargeNote"
			class="text-xs text-ink-subtle"
			data-testid="approval-too-large"
		>
			{{ tooLargeNote }}
		</p>
		<Dialog :open="open" @update:open="(value) => (open = value)">
			<DialogContent
				class="max-h-[85vh] overflow-y-auto"
				data-testid="cloud-confirm-dialog"
			>
				<DialogHeader>
					<DialogTitle>{{
						$t('taskAgent.cloudApproval.confirmForAgentRuns')
					}}</DialogTitle>
					<DialogDescription>
						{{ $t('taskAgent.cloudApproval.dialogDescription') }}
					</DialogDescription>
				</DialogHeader>
				<div v-if="fresh" class="flex flex-col gap-3 text-sm">
					<p
						v-if="warn"
						class="rounded border border-amber-500 bg-amber-50 p-2 text-xs text-amber-900 dark:bg-amber-950 dark:text-amber-200"
						data-testid="cloud-confirm-warning"
					>
						{{ $t('taskAgent.cloudApproval.invisibleWarning') }}
					</p>
					<div>
						<div class="text-2xs font-bold text-ink-subtle uppercase">
							{{ $t('taskAgent.cloudApproval.title') }}
						</div>
						<pre
							class="font-mono text-xs break-words whitespace-pre-wrap"
							data-testid="cloud-confirm-title"
						><template v-for="(piece, i) in titlePieces" :key="i"><mark v-if="piece.flagged" class="bg-amber-300 text-black">{{ piece.text }}</mark><template v-else>{{ piece.text }}</template></template></pre>
					</div>
					<div>
						<div class="text-2xs font-bold text-ink-subtle uppercase">
							{{ $t('taskAgent.cloudApproval.description') }}
						</div>
						<pre
							class="font-mono text-xs break-words whitespace-pre-wrap"
							data-testid="cloud-confirm-description"
						><template v-if="!descriptionPieces.length">{{ $t('taskAgent.ownerTexts.empty') }}</template><template v-for="(piece, i) in descriptionPieces" :key="i"><mark v-if="piece.flagged" class="bg-amber-300 text-black">{{ piece.text }}</mark><template v-else>{{ piece.text }}</template></template></pre>
					</div>
					<div>
						<div class="text-2xs font-bold text-ink-subtle uppercase">
							{{ $t('taskAgent.cloudApproval.category') }}
						</div>
						<div class="text-sm" data-testid="cloud-confirm-category">
							{{ categoryText }}
						</div>
					</div>
					<div
						v-if="shownRepositories"
						data-testid="cloud-confirm-repositories"
					>
						<div class="text-2xs font-bold text-ink-subtle uppercase">
							{{ $t('taskAgent.cloudApproval.repositories') }}
						</div>
						<ul class="font-mono text-xs">
							<li v-for="name in shownRepositories" :key="name">{{ name }}</li>
						</ul>
					</div>
					<AgentOwnerTexts :persona="shownPersona" :context="shownContext" />
					<div class="text-xs text-ink-subtle">
						{{ $t('taskAgent.cloudApproval.snapshot', { hash: shortHash }) }}
					</div>
				</div>
				<p
					v-if="error"
					class="text-sm text-red-600 dark:text-red-400"
					data-testid="cloud-confirm-error"
				>
					{{ error }}
				</p>
				<DialogFooter>
					<Button variant="outline" @click="open = false">{{
						$t('common.actions.cancel')
					}}</Button>
					<Button
						:disabled="!fresh || busy"
						data-testid="cloud-confirm-submit"
						@click="submit"
					>
						{{ $t('common.actions.confirm') }}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	</section>
</template>

<script lang="ts">
	import {
		agentRunErrorCode,
		getAgentRunsSetting,
		getTaskApproval,
		type ApprovalContext,
		type ApprovalPersona,
		type TaskApproval,
	} from '@/actions/tmgr/agentRuns';
	import { getTask, type Task } from '@/actions/tmgr/tasks';
	import AgentOwnerTexts from '@/components/tasks/AgentOwnerTexts.vue';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { contentSha256 } from '@/utils/agentApproval';
	import {
		APPROVAL_CHANGED,
		rememberAgentRunsSetting,
		signTaskApproval,
	} from '@/utils/agentApprovalClient';
	import { needsNewConfirmation } from '@/utils/agentApprovalCoverage';
	import {
		categoryLabel,
		hasInvisible,
		revokedByLabel,
		revokedReasonLabel,
		splitInvisible,
	} from '@/utils/agentApprovalPolicy';
	import { t } from '@/i18n';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
	} from 'vue';

	const MESSAGE_KEYS: Record<string, string> = {
		repositories_changed: 'taskAgent.cloudApproval.errors.repositoriesChanged',
		invalid_repositories: 'taskAgent.cloudApproval.errors.invalidRepositories',
		hash_mismatch: 'taskAgent.cloudApproval.errors.hashMismatch',
		setting_off: 'taskAgent.cloudApproval.errors.settingOff',
		ui_session_required: 'taskAgent.cloudApproval.errors.uiSessionRequired',
		signed_at_skew: 'taskAgent.cloudApproval.errors.signedAtSkew',
		signature_invalid: 'taskAgent.cloudApproval.errors.signatureInvalid',
		key_unknown: 'taskAgent.cloudApproval.errors.keyUnknown',
		category_mismatch: 'taskAgent.cloudApproval.errors.categoryMismatch',
		persona_changed: 'taskAgent.cloudApproval.errors.personaChanged',
		context_changed: 'taskAgent.cloudApproval.errors.contextChanged',
		invalid_persona: 'taskAgent.cloudApproval.errors.invalidPersona',
	};

	export default defineComponent({
		name: 'CloudAgentApproval',
		components: {
			AgentOwnerTexts,
			Button,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
		},
		props: {
			taskId: { type: Number, required: true },
			workspaceId: { type: Number, required: true },
		},
		setup(props) {
			const enabled = ref(false);
			const approval = ref<TaskApproval | null>(null);
			const open = ref(false);
			const busy = ref(false);
			const error = ref<string | null>(null);
			const fresh = ref<Task | null>(null);
			const shortHash = ref('');
			const shownRepositories = ref<string[] | null>(null);
			const shownPersona = ref<ApprovalPersona | null>(null);
			const shownContext = ref<ApprovalContext | null>(null);
			const reposChanged = ref(false);

			const refresh = async () => {
				try {
					enabled.value = await getAgentRunsSetting(props.workspaceId);
					rememberAgentRunsSetting(props.workspaceId, enabled.value);
					approval.value = enabled.value
						? await getTaskApproval(props.taskId)
						: null;
				} catch {
					approval.value = null;
				}
			};

			const onChanged = (event: Event) => {
				const detail = (event as CustomEvent).detail;
				if (detail?.taskId !== props.taskId) return;
				reposChanged.value = detail.repositoriesChanged === true;
				void refresh();
			};
			onMounted(() => {
				void refresh();
				window.addEventListener(APPROVAL_CHANGED, onChanged);
			});
			onBeforeUnmount(() =>
				window.removeEventListener(APPROVAL_CHANGED, onChanged),
			);
			watch(() => [props.taskId, props.workspaceId], refresh);

			const visible = computed(() => enabled.value && !!approval.value);

			const chipText = computed(() => {
				const a = approval.value;
				if (!a) return '';
				if (a.state === 'active')
					return t('taskAgent.cloudApproval.chip.approved');
				if (reposChanged.value && a.state === 'stale')
					return t('taskAgent.cloudApproval.chip.reposChanged');
				const reasons = a.stale_reasons ?? [];
				if (a.state === 'stale' && !reasons.includes('content')) {
					const persona = reasons.includes('persona');
					const context = reasons.includes('context');
					if (persona && context)
						return t('taskAgent.cloudApproval.chip.personaAndContext');
					if (persona) return t('taskAgent.cloudApproval.chip.persona');
					if (context) return t('taskAgent.cloudApproval.chip.context');
				}
				if (a.state === 'revoked') {
					const params = {
						reason: revokedReasonLabel(a.revoked_reason),
						kind: revokedByLabel(a.revoked_by_kind),
						name: a.revoked_by_name,
					};
					return a.revoked_by_name
						? t('taskAgent.cloudApproval.chip.revokedBy', params)
						: t('taskAgent.cloudApproval.chip.revoked', params);
				}
				return t('taskAgent.cloudApproval.chip.needsConfirmation');
			});

			const chipClass = computed(() => {
				switch (approval.value?.state) {
					case 'active':
						return 'bg-green-100 text-green-800 dark:bg-green-950 dark:text-green-300';
					case 'revoked':
						return 'bg-red-100 text-red-800 dark:bg-red-950 dark:text-red-300';
					default:
						return 'bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300';
				}
			});

			const needsConfirm = computed(() =>
				approval.value ? needsNewConfirmation(approval.value) : false,
			);

			const coverageHint = computed(() =>
				approval.value?.state === 'active' && needsConfirm.value
					? t('taskAgent.cloudApproval.coverageHint')
					: '',
			);

			const tooLargeNote = computed(() => {
				const a = approval.value;
				if (!a) return '';
				const parts = [
					a.persona_too_large ? t('taskAgent.cloudApproval.partPersona') : '',
					a.context_too_large ? t('taskAgent.cloudApproval.partContext') : '',
				].filter(Boolean);
				return parts.length
					? t('taskAgent.cloudApproval.tooLarge', {
							parts:
								parts.length > 1
									? t('taskAgent.cloudApproval.partsJoin', {
											first: parts[0],
											second: parts[1],
										})
									: parts[0],
						})
					: '';
			});

			const titlePieces = computed(() =>
				splitInvisible(fresh.value?.title ?? ''),
			);
			const descriptionPieces = computed(() =>
				splitInvisible(fresh.value?.description ?? ''),
			);
			const categoryText = computed(() =>
				fresh.value ? categoryLabel(fresh.value as any) : '',
			);
			const warn = computed(
				() =>
					hasInvisible(fresh.value?.title ?? '') ||
					hasInvisible(fresh.value?.description ?? '') ||
					hasInvisible(shownPersona.value?.name ?? '') ||
					hasInvisible(shownPersona.value?.instructions ?? '') ||
					(shownContext.value?.pages ?? []).some(
						(page) => hasInvisible(page.title) || hasInvisible(page.body),
					),
			);

			const openDialog = async () => {
				error.value = null;
				fresh.value = null;
				shownRepositories.value = null;
				shownPersona.value = null;
				shownContext.value = null;
				open.value = true;
				try {
					const current = await getTaskApproval(props.taskId);
					shownRepositories.value = current.repositories ?? null;
					shownPersona.value = current.persona ?? null;
					shownContext.value = current.context ?? null;
					const task = await getTask(props.taskId, false);
					fresh.value = task;
					shortHash.value = (
						await contentSha256(task.title, task.description)
					).slice(0, 12);
				} catch (e) {
					error.value = e instanceof Error ? e.message : String(e);
				}
			};

			const submit = async () => {
				if (!fresh.value) return;
				busy.value = true;
				error.value = null;
				try {
					await signTaskApproval(
						{
							id: props.taskId,
							title: fresh.value.title,
							description: fresh.value.description,
							workspace_id: props.workspaceId,
							project_category_id: fresh.value.project_category_id,
							category: fresh.value.category as any,
						},
						'confirm',
						shownRepositories.value,
						{ persona: shownPersona.value, context: shownContext.value },
					);
					open.value = false;
					reposChanged.value = false;
					await refresh();
				} catch (e) {
					const code = agentRunErrorCode(e);
					error.value =
						(code && MESSAGE_KEYS[code] && t(MESSAGE_KEYS[code])) ||
						(e instanceof Error ? e.message : String(e));
					if (
						code === 'hash_mismatch' ||
						code === 'repositories_changed' ||
						code === 'persona_changed' ||
						code === 'context_changed'
					)
						fresh.value = null;
				} finally {
					busy.value = false;
				}
			};

			return {
				visible,
				approval,
				chipText,
				chipClass,
				open,
				busy,
				error,
				fresh,
				shortHash,
				shownRepositories,
				shownPersona,
				shownContext,
				needsConfirm,
				coverageHint,
				tooLargeNote,
				titlePieces,
				descriptionPieces,
				warn,
				categoryText,
				openDialog,
				submit,
			};
		},
	});
</script>
