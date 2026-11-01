<template>
	<div class="flex items-center gap-2">
		<Button
			type="button"
			variant="ghost"
			size="sm"
			class="h-7 gap-1 px-2 text-xs text-ink-muted hover:bg-surface-hover hover:text-ink"
			data-testid="ai-rewrite-open"
			:disabled="!canRewrite"
			:title="disabledReason || 'Rewrite the description with AI'"
			@click="open"
		>
			<Sparkles class="h-3.5 w-3.5" />
			Rewrite with AI
		</Button>
		<Button
			v-if="undoVisible"
			type="button"
			variant="ghost"
			size="sm"
			class="h-7 gap-1 px-2 text-xs text-ink-muted hover:bg-surface-hover hover:text-ink"
			data-testid="ai-rewrite-undo"
			@click="undo"
		>
			<Undo2 class="h-3.5 w-3.5" />
			Undo AI rewrite
		</Button>

		<Dialog :open="isOpen" @update:open="onOpenChange">
			<DialogContent
				class="flex max-h-[90vh] w-[calc(100vw-1.5rem)] max-w-5xl flex-col gap-3 overflow-hidden rounded-card border border-line bg-surface p-4 text-ink sm:p-6"
			>
				<DialogHeader>
					<DialogTitle class="text-ink">Rewrite with AI</DialogTitle>
					<DialogDescription class="text-ink-muted">
						Review the proposed text. Nothing changes until you accept it.
					</DialogDescription>
				</DialogHeader>

				<div
					class="flex flex-wrap gap-1.5"
					role="tablist"
					aria-label="Revisions"
				>
					<button
						v-for="(_, index) in session.revisions"
						:key="index"
						type="button"
						role="tab"
						:aria-selected="index === session.selected"
						:class="[
							'rounded-pill border px-3 py-1 text-xs transition',
							index === session.selected
								? 'border-brand bg-brand-bg text-brand'
								: 'border-line text-ink-muted hover:bg-surface-hover hover:text-ink',
						]"
						@click="select(index)"
					>
						{{ index === 0 ? 'Original' : `Revision ${index}` }}
					</button>
				</div>

				<div
					class="grid min-h-0 flex-1 grid-cols-1 gap-3 overflow-y-auto md:grid-cols-2"
				>
					<section class="flex min-h-0 min-w-0 flex-col gap-1">
						<h3
							class="text-2xs font-bold tracking-wide text-ink-subtle uppercase"
						>
							Original
						</h3>
						<div
							class="max-h-40 min-h-[4rem] overflow-y-auto rounded-md border border-line bg-surface-sunken px-3 py-2 md:max-h-[50vh]"
							data-testid="ai-rewrite-original"
						>
							<DescriptionMarkdown :content="session.revisions[0]" />
						</div>
					</section>
					<section class="flex min-h-0 min-w-0 flex-col gap-1">
						<h3
							class="text-2xs font-bold tracking-wide text-ink-subtle uppercase"
						>
							{{ previewLabel }}
						</h3>
						<div
							class="relative max-h-[40vh] min-h-[6rem] overflow-y-auto rounded-md border border-line bg-surface-sunken px-3 py-2 md:max-h-[50vh]"
							data-testid="ai-rewrite-preview"
							:aria-busy="loading"
						>
							<p
								v-if="loading"
								class="flex items-center gap-2 text-sm text-ink-muted"
								role="status"
							>
								<Loader2 class="h-4 w-4 animate-spin" />
								Rewriting…
							</p>
							<DescriptionMarkdown v-else :content="selectedText(session)" />
						</div>
					</section>
				</div>

				<div
					v-if="error"
					role="alert"
					class="flex flex-wrap items-center gap-x-3 rounded-md border border-line bg-surface-sunken px-3 py-2 text-sm"
					data-testid="ai-rewrite-error"
				>
					<span>{{ error }}</span>
					<button
						type="button"
						class="underline hover:text-ink"
						:disabled="loading"
						@click="run(lastInstruction)"
					>
						Try again
					</button>
				</div>

				<form class="flex flex-col gap-2 sm:flex-row" @submit.prevent="refine">
					<Input
						v-model="instruction"
						class="min-w-0 flex-1"
						placeholder="Refine: e.g. make it shorter, add acceptance criteria"
						aria-label="Refine instruction"
						data-testid="ai-rewrite-instruction"
						:disabled="loading"
					/>
					<Button
						type="submit"
						variant="outline"
						:disabled="loading || !instruction.trim()"
						data-testid="ai-rewrite-refine"
					>
						Refine
					</Button>
				</form>

				<div class="flex justify-end gap-2">
					<Button
						type="button"
						variant="ghost"
						class="text-ink-muted hover:bg-surface-hover hover:text-ink"
						data-testid="ai-rewrite-cancel"
						@click="close"
					>
						Cancel
					</Button>
					<Button
						type="button"
						class="bg-brand text-on-accent hover:bg-brand-hover"
						data-testid="ai-rewrite-accept"
						:disabled="loading || session.selected === 0"
						@click="accept"
					>
						Accept
					</Button>
				</div>
			</DialogContent>
		</Dialog>
	</div>
</template>

<script setup lang="ts">
	import { optimizeWithAI } from '@/actions/tmgr/tasks';
	import DescriptionMarkdown from '@/components/description/DescriptionMarkdown.vue';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { Input } from '@/components/ui/input';
	import {
		acceptRevision,
		addRevision,
		composeRewriteText,
		createRequestGuard,
		createRewriteSession,
		rewriteErrorMessage,
		selectedText,
		selectRevision,
	} from '@/utils/aiRewrite';
	import { Loader2, Sparkles, Undo2 } from 'lucide-vue-next';
	import { computed, onBeforeUnmount, ref } from 'vue';

	const props = defineProps<{
		description: string;
		disabledReason?: string;
	}>();
	const emit = defineEmits<{ apply: [text: string] }>();

	const guard = createRequestGuard();
	const isOpen = ref(false);
	const session = ref(createRewriteSession(''));
	const loading = ref(false);
	const error = ref('');
	const instruction = ref('');
	const lastInstruction = ref('');
	const undoText = ref<string | null>(null);
	const acceptedText = ref('');

	const canRewrite = computed(
		() => !props.disabledReason && props.description.trim() !== '',
	);
	const disabledReason = computed(() =>
		props.disabledReason
			? props.disabledReason
			: props.description.trim()
				? ''
				: 'Write a description first',
	);
	const previewLabel = computed(() =>
		session.value.selected === 0
			? 'Preview: original'
			: `Preview: revision ${session.value.selected}`,
	);
	const undoVisible = computed(
		() => undoText.value !== null && props.description === acceptedText.value,
	);

	const run = async (ask: string) => {
		const token = guard.begin();
		lastInstruction.value = ask;
		loading.value = true;
		error.value = '';
		try {
			const result = await optimizeWithAI(
				composeRewriteText(selectedText(session.value), ask),
			);
			if (!guard.isCurrent(token)) return;
			session.value = addRevision(session.value, String(result ?? ''));
			instruction.value = '';
		} catch (e) {
			if (!guard.isCurrent(token)) return;
			error.value = rewriteErrorMessage(e);
		} finally {
			if (guard.isCurrent(token)) loading.value = false;
		}
	};

	const open = () => {
		session.value = createRewriteSession(props.description);
		instruction.value = '';
		error.value = '';
		isOpen.value = true;
		run('');
	};

	const close = () => {
		guard.cancel();
		loading.value = false;
		isOpen.value = false;
	};

	const onOpenChange = (value: boolean) => {
		if (!value) close();
	};

	const select = (index: number) => {
		session.value = selectRevision(session.value, index);
	};

	const refine = () => {
		if (instruction.value.trim() && !loading.value) run(instruction.value);
	};

	const accept = () => {
		const accepted = acceptRevision(session.value, props.description);
		if (!accepted) {
			error.value =
				'The description changed while this was open. Close and rewrite it again.';
			return;
		}
		undoText.value = accepted.previous;
		acceptedText.value = accepted.next;
		emit('apply', accepted.next);
		close();
	};

	const undo = () => {
		if (undoText.value === null) return;
		emit('apply', undoText.value);
		undoText.value = null;
	};

	onBeforeUnmount(() => guard.cancel());
</script>
