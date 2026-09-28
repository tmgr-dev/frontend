<template>
	<form class="flex flex-col gap-3" @submit.prevent="$emit('submit')">
		<label class="flex flex-col gap-1">
			<span class="text-sm font-medium">Name</span>
			<Input
				:model-value="modelValue.name"
				maxlength="60"
				placeholder="Reviewer"
				@update:model-value="(v) => update('name', String(v))"
			/>
			<span v-if="fieldErrors.name" class="text-xs text-destructive">
				{{ fieldErrors.name.join(' ') }}
			</span>
		</label>

		<label class="flex flex-col gap-1">
			<span class="text-sm font-medium">Description</span>
			<Textarea
				:model-value="modelValue.description"
				maxlength="500"
				rows="2"
				placeholder="What this persona is for"
				@update:model-value="(v) => update('description', String(v))"
			/>
			<span
				v-if="fieldErrors.description"
				class="text-xs text-destructive"
			>
				{{ fieldErrors.description.join(' ') }}
			</span>
		</label>

		<label class="flex flex-col gap-1">
			<span class="text-sm font-medium">System prompt (Markdown)</span>
			<Textarea
				:model-value="modelValue.system_prompt"
				rows="6"
				class="font-mono text-xs"
				placeholder="You are..."
				@update:model-value="(v) => update('system_prompt', String(v))"
			/>
			<span
				:class="[
					'text-xs',
					promptOverLimit
						? 'text-destructive'
						: 'text-muted-foreground',
				]"
			>
				{{ promptBytes }} / {{ PROMPT_MAX_BYTES }} bytes
			</span>
			<span
				v-if="fieldErrors.system_prompt"
				class="text-xs text-destructive"
			>
				{{ fieldErrors.system_prompt.join(' ') }}
			</span>
		</label>

		<div class="flex gap-2">
			<Button type="submit" size="sm" :disabled="saving || promptOverLimit">
				{{ submitLabel }}
			</Button>
			<Button
				v-if="showCancel"
				type="button"
				variant="outline"
				size="sm"
				@click="$emit('cancel')"
			>
				Cancel
			</Button>
		</div>
	</form>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Textarea } from '@/components/ui/textarea';
	import { byteLength, PROMPT_MAX_BYTES } from '@/utils/personas';
	import { computed, defineComponent, type PropType } from 'vue';

	export interface PersonaFormModel {
		name: string;
		description: string;
		system_prompt: string;
	}

	export default defineComponent({
		name: 'PersonaForm',
		components: { Button, Input, Textarea },
		props: {
			modelValue: { type: Object as PropType<PersonaFormModel>, required: true },
			fieldErrors: {
				type: Object as PropType<Record<string, string[]>>,
				default: () => ({}),
			},
			saving: { type: Boolean, default: false },
			submitLabel: { type: String, default: 'Save' },
			showCancel: { type: Boolean, default: false },
		},
		emits: ['update:modelValue', 'submit', 'cancel'],
		setup(props, { emit }) {
			const update = (key: keyof PersonaFormModel, value: string) => {
				emit('update:modelValue', { ...props.modelValue, [key]: value });
			};
			const promptBytes = computed(() =>
				byteLength(props.modelValue.system_prompt),
			);
			const promptOverLimit = computed(
				() => promptBytes.value > PROMPT_MAX_BYTES,
			);

			return { update, promptBytes, promptOverLimit, PROMPT_MAX_BYTES };
		},
	});
</script>
