<template>
	<section
		v-if="show"
		class="flex flex-col gap-2"
		data-testid="confirm-for-agent-runs"
	>
		<div class="flex flex-wrap items-center gap-2">
			<span class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
				>Agent runs</span
			>
			<span class="flex-1" />
			<Button
				variant="outline"
				size="sm"
				data-testid="confirm-open"
				@click="openDialog"
			>
				Confirm for agent runs
			</Button>
		</div>
		<p v-if="done" class="text-xs text-ink-subtle" data-testid="confirm-done">
			Confirmed. Agents may run on this exact text.
		</p>
		<Dialog :open="open" @update:open="(value) => (open = value)">
			<DialogContent
				class="max-h-[85vh] overflow-y-auto"
				data-testid="confirm-dialog"
			>
				<DialogHeader>
					<DialogTitle>Confirm for agent runs</DialogTitle>
					<DialogDescription>
						This is everything an agent will read from this task. Confirm only
						if you wrote or checked all of it. Any later change by someone or
						something else asks for a new confirmation.
					</DialogDescription>
				</DialogHeader>
				<div v-if="preview" class="flex flex-col gap-3 text-sm">
					<div>
						<div class="text-2xs font-bold uppercase text-ink-subtle">
							Title
						</div>
						<div class="whitespace-pre-wrap break-words">
							{{ preview.title }}
						</div>
					</div>
					<div>
						<div class="text-2xs font-bold uppercase text-ink-subtle">
							Description
						</div>
						<pre class="whitespace-pre-wrap break-words font-sans">{{
							preview.description || '(empty)'
						}}</pre>
					</div>
					<div v-if="preview.descriptionJson">
						<div class="text-2xs font-bold uppercase text-ink-subtle">
							Description (editor data)
						</div>
						<pre class="whitespace-pre-wrap break-words font-mono text-xs">{{
							preview.descriptionJson
						}}</pre>
					</div>
					<div v-if="preview.checkpoints.length">
						<div class="text-2xs font-bold uppercase text-ink-subtle">
							Checkpoints
						</div>
						<ul class="list-none">
							<li
								v-for="(line, index) in preview.checkpoints"
								:key="index"
								class="whitespace-pre-wrap break-words"
							>
								{{ line }}
							</li>
						</ul>
						<div class="mt-1 text-2xs text-ink-subtle">
							Stored checkpoints, in full:
						</div>
						<pre
							class="whitespace-pre-wrap break-words font-mono text-xs"
							data-testid="confirm-checkpoints-raw"
							>{{ preview.checkpointsText }}</pre
						>
					</div>
					<div v-if="preview.comments.length">
						<div class="text-2xs font-bold uppercase text-ink-subtle">
							Comments
						</div>
						<div
							v-for="comment in preview.comments"
							:key="comment.id"
							class="mt-1 rounded border border-line p-2"
						>
							<div class="text-xs text-ink-subtle">
								{{ comment.author_kind
								}}<template v-if="comment.author_name">
									· {{ comment.author_name }}</template
								>
							</div>
							<div class="whitespace-pre-wrap break-words">
								{{ comment.message }}
							</div>
						</div>
					</div>
					<div class="text-xs text-ink-subtle" data-testid="confirm-hash">
						Snapshot {{ preview.shortHash }}
					</div>
				</div>
				<p
					v-if="error"
					class="text-sm text-red-600 dark:text-red-400"
					data-testid="confirm-error"
				>
					{{ error }}
				</p>
				<DialogFooter>
					<Button variant="outline" @click="open = false">Cancel</Button>
					<Button
						:disabled="!preview || busy"
						data-testid="confirm-submit"
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
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import {
		buildConfirmPreview,
		confirmShownTask,
		type ConfirmPreview,
	} from '@/local/confirmPreview';
	import { activeLocalWorkspace, localContext } from '@/local/runtime';
	import store from '@/store';
	import { isDesktopApp } from '@/utils/desktop';
	import { defineComponent, ref } from 'vue';

	export default defineComponent({
		name: 'ConfirmForAgentRuns',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
		},
		props: { taskId: { type: Number, required: true } },
		setup(props) {
			const workspace = activeLocalWorkspace();
			const show = isDesktopApp() && !!workspace;
			const open = ref(false);
			const busy = ref(false);
			const done = ref(false);
			const error = ref<string | null>(null);
			const preview = ref<ConfirmPreview | null>(null);

			const context = () =>
				localContext(workspace!, {
					id: Number(store.state.user?.id) || 0,
					name: store.state.user?.name ?? '',
					email: store.state.user?.email ?? '',
				});

			const openDialog = async () => {
				error.value = null;
				done.value = false;
				preview.value = null;
				open.value = true;
				try {
					preview.value = await buildConfirmPreview(
						(
							await context()
						).db,
						props.taskId,
					);
					if (!preview.value) error.value = 'Task not found.';
				} catch (e) {
					error.value = e instanceof Error ? e.message : String(e);
				}
			};

			const submit = async () => {
				if (!preview.value) return;
				busy.value = true;
				error.value = null;
				try {
					await confirmShownTask(await context(), preview.value);
					done.value = true;
					open.value = false;
				} catch (e) {
					error.value =
						'The task changed since it was shown. Reopen this dialog to review it again.';
					preview.value = null;
				} finally {
					busy.value = false;
				}
			};

			return { show, open, busy, done, error, preview, openDialog, submit };
		},
	});
</script>
