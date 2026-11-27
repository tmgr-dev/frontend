<template>
	<Dialog :open="true" @update:open="(open) => !open && dismiss()">
		<DialogContent @close-auto-focus.prevent>
			<DialogHeader>
				<DialogTitle>{{
					$t('board.outcome.title', { title: task.title })
				}}</DialogTitle>
				<DialogDescription>{{
					$t('board.outcome.description')
				}}</DialogDescription>
			</DialogHeader>
			<div class="flex flex-wrap gap-2">
				<Button
					v-for="preset in presets"
					:key="preset"
					variant="outline"
					size="sm"
					:disabled="sending"
					@click="draft = $t('board.outcome.' + preset)"
					>{{ $t('board.outcome.' + preset) }}</Button
				>
			</div>
			<label class="grid gap-2"
				>{{ $t('board.outcome.note')
				}}<Textarea v-model="draft" :disabled="sending"
			/></label>
			<p v-if="error" role="alert">{{ $t(error) }}</p>
			<DialogFooter>
				<Button variant="outline" :disabled="sending" @click="dismiss">{{
					$t('board.outcome.skip')
				}}</Button>
				<Button :disabled="sending || !draft.trim()" @click="post">{{
					$t('board.outcome.post')
				}}</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>
<script>
	import { defineComponent } from 'vue';
	import { Button } from '@/components/ui/button';
	import { Textarea } from '@/components/ui/textarea';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { createComment } from '@/actions/tmgr/comments';
	export default defineComponent({
		components: {
			Button,
			Textarea,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
		},
		props: { task: { type: Object, required: true } },
		emits: ['dismiss'],
		data: () => ({
			draft: '',
			sending: false,
			controller: new AbortController(),
			error: '',
			presets: ['done', 'declined', 'duplicate', 'moved'],
		}),
		beforeUnmount() {
			this.controller.abort();
		},
		methods: {
			dismiss() {
				if (!this.sending) this.$emit('dismiss', this.task.sequence);
			},
			async post() {
				const message = this.draft.trim();
				if (this.sending || !message) return;
				this.sending = true;
				this.error = '';
				try {
					await createComment(
						this.task.id,
						{ message },
						{
							signal: this.controller.signal,
							params: { workspace_id: this.task.workspaceId },
							headers: { 'X-Workspace-Id': String(this.task.workspaceId) },
						},
					);
					this.$emit('dismiss', this.task.sequence, true);
				} catch (error) {
					this.error =
						!error.response || error.response.status >= 500
							? 'board.outcome.uncertain'
							: 'board.outcome.failed';
				} finally {
					this.sending = false;
				}
			},
		},
	});
</script>
