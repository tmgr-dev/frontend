<template>
	<form class="flex flex-col gap-3" @submit.prevent="$emit('submit')">
		<fieldset v-if="showScope" class="flex flex-col gap-1.5">
			<legend class="mb-1 text-sm font-medium">
				{{ $t('settingsPages.personaForm.whoCanAssign') }}
			</legend>
			<label class="flex items-center gap-2 text-sm">
				<input
					type="radio"
					name="persona-scope"
					value="account"
					class="accent-primary"
					:checked="modelValue.scope === 'account'"
					@change="update('scope', 'account')"
				/>
				{{ $t('settingsPages.personaForm.onlyMe') }}
				<span class="text-xs text-muted-foreground">
					{{ $t('settingsPages.personaForm.onlyMeHint') }}
				</span>
			</label>
			<label class="flex items-center gap-2 text-sm">
				<input
					type="radio"
					name="persona-scope"
					value="workspace"
					class="accent-primary"
					:checked="modelValue.scope === 'workspace'"
					@change="update('scope', 'workspace')"
				/>
				{{ $t('settingsPages.personaForm.thisWorkspace')
				}}{{ workspaceName ? ` (${workspaceName})` : '' }}
				<span class="text-xs text-muted-foreground">
					{{ $t('settingsPages.personaForm.thisWorkspaceHint') }}
				</span>
			</label>
		</fieldset>

		<label class="flex flex-col gap-1">
			<span class="text-sm font-medium">{{
				$t('settingsPages.personaForm.name')
			}}</span>
			<Input
				:model-value="modelValue.name"
				maxlength="60"
				:placeholder="$t('settingsPages.personaForm.namePlaceholder')"
				@update:model-value="(v) => update('name', String(v))"
			/>
			<span v-if="fieldErrors.name" class="text-xs text-destructive">
				{{ fieldErrors.name.join(' ') }}
			</span>
		</label>

		<label class="flex flex-col gap-1">
			<span class="text-sm font-medium">{{
				$t('settingsPages.personaForm.description')
			}}</span>
			<Textarea
				:model-value="modelValue.description"
				maxlength="500"
				rows="2"
				:placeholder="$t('settingsPages.personaForm.descriptionPlaceholder')"
				@update:model-value="(v) => update('description', String(v))"
			/>
			<span v-if="fieldErrors.description" class="text-xs text-destructive">
				{{ fieldErrors.description.join(' ') }}
			</span>
		</label>

		<label class="flex flex-col gap-1">
			<span class="text-sm font-medium">{{
				$t('settingsPages.personaForm.systemPrompt')
			}}</span>
			<Textarea
				:model-value="modelValue.system_prompt"
				rows="6"
				class="font-mono text-xs"
				:placeholder="$t('settingsPages.personaForm.systemPromptPlaceholder')"
				@update:model-value="(v) => update('system_prompt', String(v))"
			/>
			<span
				:class="[
					'text-xs',
					promptOverLimit ? 'text-destructive' : 'text-muted-foreground',
				]"
			>
				{{
					$t('settingsPages.personaForm.bytes', {
						used: promptBytes,
						max: PROMPT_MAX_BYTES,
					})
				}}
			</span>
			<span v-if="fieldErrors.system_prompt" class="text-xs text-destructive">
				{{ fieldErrors.system_prompt.join(' ') }}
			</span>
		</label>

		<label class="flex flex-col gap-1">
			<span class="text-sm font-medium">
				{{ $t('settingsPages.personaForm.notify') }}
			</span>
			<Select
				:model-value="modelValue.owner_notifications"
				@update:model-value="(v) => update('owner_notifications', String(v))"
			>
				<SelectTrigger :aria-label="$t('settingsPages.personaForm.notify')">
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value="all">{{
						$t('settingsPages.personaForm.notifyAll')
					}}</SelectItem>
					<SelectItem value="mentions">{{
						$t('settingsPages.personaForm.notifyMentions')
					}}</SelectItem>
					<SelectItem value="none">{{
						$t('settingsPages.personaForm.notifyNone')
					}}</SelectItem>
				</SelectContent>
			</Select>
			<span
				v-if="fieldErrors.owner_notifications"
				class="text-xs text-destructive"
			>
				{{ fieldErrors.owner_notifications.join(' ') }}
			</span>
		</label>

		<div class="flex gap-2">
			<Button type="submit" size="sm" :disabled="saving || promptOverLimit">
				{{ submitLabel || $t('common.actions.save') }}
			</Button>
			<Button
				v-if="showCancel"
				type="button"
				variant="outline"
				size="sm"
				@click="$emit('cancel')"
			>
				{{ $t('common.actions.cancel') }}
			</Button>
		</div>
	</form>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from '@/components/ui/select';
	import { Textarea } from '@/components/ui/textarea';
	import type { PersonaOwnerNotifications } from '@/actions/tmgr/personas';
	import { byteLength, PROMPT_MAX_BYTES } from '@/utils/personas';
	import { computed, defineComponent, type PropType } from 'vue';

	export interface PersonaFormModel {
		name: string;
		description: string;
		system_prompt: string;
		scope: 'account' | 'workspace';
		owner_notifications: PersonaOwnerNotifications;
	}

	export default defineComponent({
		name: 'PersonaForm',
		components: {
			Button,
			Input,
			Select,
			SelectContent,
			SelectItem,
			SelectTrigger,
			SelectValue,
			Textarea,
		},
		props: {
			modelValue: {
				type: Object as PropType<PersonaFormModel>,
				required: true,
			},
			fieldErrors: {
				type: Object as PropType<Record<string, string[]>>,
				default: () => ({}),
			},
			saving: { type: Boolean, default: false },
			submitLabel: { type: String, default: '' },
			showCancel: { type: Boolean, default: false },
			showScope: { type: Boolean, default: false },
			workspaceName: { type: String, default: '' },
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
