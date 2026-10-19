<template>
	<section
		v-if="visible"
		class="flex flex-col gap-2"
		data-testid="cloud-agent-approval"
	>
		<div class="flex flex-wrap items-center gap-2">
			<span class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
				>Agent runs</span
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
				Confirm for agent runs
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
					<DialogTitle>Confirm for agent runs</DialogTitle>
					<DialogDescription>
						This is exactly what an agent will read from this task. Confirm only
						if you wrote or checked all of it. Any later change by someone or
						something else cancels the confirmation.
					</DialogDescription>
				</DialogHeader>
				<div v-if="fresh" class="flex flex-col gap-3 text-sm">
					<p
						v-if="warn"
						class="rounded border border-amber-500 bg-amber-50 p-2 text-xs text-amber-900 dark:bg-amber-950 dark:text-amber-200"
						data-testid="cloud-confirm-warning"
					>
						This text contains invisible or direction-changing characters. They
						are shown highlighted as code points. Do not confirm unless you
						expect them.
					</p>
					<div>
						<div class="text-2xs font-bold uppercase text-ink-subtle">
							Title
						</div>
						<pre
							class="whitespace-pre-wrap break-words font-mono text-xs"
							data-testid="cloud-confirm-title"
						><template v-for="(piece, i) in titlePieces" :key="i"><mark v-if="piece.flagged" class="bg-amber-300 text-black">{{ piece.text }}</mark><template v-else>{{ piece.text }}</template></template></pre>
					</div>
					<div>
						<div class="text-2xs font-bold uppercase text-ink-subtle">
							Description
						</div>
						<pre
							class="whitespace-pre-wrap break-words font-mono text-xs"
							data-testid="cloud-confirm-description"
						><template v-if="!descriptionPieces.length">(empty)</template><template v-for="(piece, i) in descriptionPieces" :key="i"><mark v-if="piece.flagged" class="bg-amber-300 text-black">{{ piece.text }}</mark><template v-else>{{ piece.text }}</template></template></pre>
					</div>
					<div>
						<div class="text-2xs font-bold uppercase text-ink-subtle">
							Category
						</div>
						<div class="text-sm" data-testid="cloud-confirm-category">
							{{ categoryText }}
						</div>
					</div>
					<div
						v-if="shownRepositories"
						data-testid="cloud-confirm-repositories"
					>
						<div class="text-2xs font-bold uppercase text-ink-subtle">
							Repositories
						</div>
						<ul class="font-mono text-xs">
							<li v-for="name in shownRepositories" :key="name">{{ name }}</li>
						</ul>
					</div>
					<AgentOwnerTexts :persona="shownPersona" :context="shownContext" />
					<div class="text-xs text-ink-subtle">Snapshot {{ shortHash }}</div>
				</div>
				<p
					v-if="error"
					class="text-sm text-red-600 dark:text-red-400"
					data-testid="cloud-confirm-error"
				>
					{{ error }}
				</p>
				<DialogFooter>
					<Button variant="outline" @click="open = false">Cancel</Button>
					<Button
						:disabled="!fresh || busy"
						data-testid="cloud-confirm-submit"
						@click="submit"
					>
						Confirm
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
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
	} from 'vue';

	const MESSAGES: Record<string, string> = {
		repositories_changed:
			'The repositories of this category changed. Reopen this dialog.',
		invalid_repositories:
			'The repositories of this category are not valid. Fix them in the category settings.',
		hash_mismatch:
			'The task changed since it was shown. Reopen this dialog to review it again.',
		setting_off:
			'Agent runs are turned off for you in this workspace (Settings, Modules).',
		ui_session_required: 'Confirm from the web or desktop app.',
		signed_at_skew: 'Your clock is off by more than ten minutes.',
		signature_invalid: 'The signature was rejected. Try again.',
		key_unknown: 'The signing key was not accepted. Try again.',
		category_mismatch: 'The task category changed. Reopen this dialog.',
		persona_changed: 'The persona instructions changed. Reopen this dialog.',
		context_changed: 'The workspace rules changed. Reopen this dialog.',
		invalid_persona: 'The persona cannot be signed. Reopen this dialog.',
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
				if (a.state === 'active') return 'Approved for agent runs';
				if (reposChanged.value && a.state === 'stale')
					return 'Needs confirmation: the repositories of this category changed';
				const reasons = a.stale_reasons ?? [];
				if (a.state === 'stale' && !reasons.includes('content')) {
					const persona = reasons.includes('persona');
					const context = reasons.includes('context');
					if (persona && context)
						return 'Needs confirmation: the persona instructions and the workspace rules changed';
					if (persona)
						return 'Needs confirmation: the persona instructions changed';
					if (context) return 'Needs confirmation: the workspace rules changed';
				}
				if (a.state === 'revoked') {
					const who = a.revoked_by_name ? ` by ${a.revoked_by_name}` : '';
					return `Approval cancelled: ${revokedReasonLabel(
						a.revoked_reason,
					)}${who} via ${revokedByLabel(a.revoked_by_kind)}`;
				}
				return 'Needs confirmation';
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
					? 'Confirm again to give agents the persona instructions and workspace rules.'
					: '',
			);

			const tooLargeNote = computed(() => {
				const a = approval.value;
				if (!a) return '';
				const parts = [
					a.persona_too_large ? 'the persona instructions' : '',
					a.context_too_large ? 'the workspace rules' : '',
				].filter(Boolean);
				return parts.length
					? `Agents do not get ${parts.join(' or ')}: too large to send.`
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
						(code && MESSAGES[code]) ||
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
