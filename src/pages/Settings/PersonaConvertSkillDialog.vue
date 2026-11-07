<template>
	<Dialog :open="open" @update:open="onOpenChange">
		<DialogContent class="sm:max-w-[480px]">
			<DialogHeader>
				<DialogTitle>Convert to shared</DialogTitle>
			</DialogHeader>

			<p v-if="loading" class="text-sm text-muted-foreground">Loading…</p>
			<p v-else-if="loadFailed" class="text-sm text-destructive">
				Could not load the conversion details.
			</p>
			<p
				v-else-if="preview?.library_clash"
				class="text-sm"
				data-testid="convert-clash"
			>
				Your skills library already has a skill with the slug “{{ slug }}”.
				Delete or rename one of them first.
			</p>
			<template v-else-if="preview">
				<p class="text-sm">
					Moves this skill to your skills library and attaches it to
					{{ personaName }}. Agents keep the same instructions.
				</p>
				<div
					v-if="preview.other_personas.length"
					class="flex flex-col gap-2"
					data-testid="convert-others"
				>
					<label
						v-for="other in preview.other_personas"
						:key="other.id"
						class="flex items-start gap-2 text-sm"
					>
						<input
							type="checkbox"
							class="mt-1 accent-primary"
							:data-testid="`convert-replace-${other.id}`"
							:checked="checked.has(other.id)"
							:disabled="busy"
							@change="
								(e) => toggle(other.id, (e.target as HTMLInputElement).checked)
							"
						/>
						<span class="flex flex-col gap-1">
							<span>
								Also replace the copy on {{ other.name }}
								<span v-if="other.archived" class="text-muted-foreground">
									(archived)
								</span>
							</span>
							<span
								:class="[
									'w-fit rounded px-1.5 py-0.5 text-2xs',
									other.same_body
										? 'bg-muted text-muted-foreground'
										: 'bg-amber-100 text-amber-900 dark:bg-amber-900/30 dark:text-amber-200',
								]"
							>
								{{
									other.same_body
										? 'Same text'
										: 'Different text — will be replaced'
								}}
							</span>
						</span>
					</label>
				</div>
			</template>

			<DialogFooter>
				<Button
					variant="outline"
					data-testid="convert-close"
					:disabled="busy"
					@click="close"
				>
					{{ preview?.library_clash ? 'Close' : 'Cancel' }}
				</Button>
				<Button
					v-if="preview && !preview.library_clash"
					data-testid="convert-confirm"
					:disabled="busy"
					@click="confirm"
				>
					Convert
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import {
		convertSkillToShared,
		previewConvertSkill,
		type ConvertPreview,
	} from '@/actions/tmgr/personas';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { toast } from '@/components/ui/toast';
	import { buildReplacePersonas, convertSuccessTitle } from '@/utils/personas';
	import { defineComponent, ref, watch } from 'vue';

	export default defineComponent({
		name: 'PersonaConvertSkillDialog',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogFooter,
			DialogHeader,
			DialogTitle,
		},
		props: {
			open: { type: Boolean, required: true },
			personaId: { type: String, required: true },
			personaName: { type: String, required: true },
			slug: { type: String, required: true },
		},
		emits: ['update:open', 'converted'],
		setup(props, { emit }) {
			const preview = ref<ConvertPreview | null>(null);
			const loading = ref(false);
			const loadFailed = ref(false);
			const busy = ref(false);
			const checked = ref(new Set<string>());

			const load = async () => {
				preview.value = null;
				loadFailed.value = false;
				checked.value = new Set();
				loading.value = true;
				try {
					preview.value = await previewConvertSkill(
						props.personaId,
						props.slug,
					);
				} catch {
					loadFailed.value = true;
				} finally {
					loading.value = false;
				}
			};

			watch(
				() => props.open,
				(isOpen) => {
					if (isOpen) load();
				},
				{ immediate: true },
			);

			const close = () => emit('update:open', false);
			const onOpenChange = (value: boolean) => {
				if (busy.value && !value) return;
				emit('update:open', value);
			};

			const toggle = (id: string, on: boolean) => {
				const next = new Set(checked.value);
				if (on) next.add(id);
				else next.delete(id);
				checked.value = next;
			};

			const confirm = async () => {
				if (busy.value || !preview.value) return;
				busy.value = true;
				try {
					const result = await convertSkillToShared(
						props.personaId,
						props.slug,
						buildReplacePersonas(preview.value.other_personas, checked.value),
					);
					toast({
						title: convertSuccessTitle(result.replaced_personas?.length ?? 0),
					});
					emit('update:open', false);
					emit('converted', props.slug);
				} catch (error: any) {
					toast({
						title: 'Could not convert this skill',
						description: error?.response?.data?.message,
						variant: 'destructive',
					});
				} finally {
					busy.value = false;
				}
			};

			return {
				preview,
				loading,
				loadFailed,
				busy,
				checked,
				close,
				onOpenChange,
				toggle,
				confirm,
			};
		},
	});
</script>
