<script lang="ts">
	import {
		AlertDialog,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { isConfirmNameMatch } from '@/utils/confirmName';
	import { computed, defineComponent, ref, watch } from 'vue';

	export default defineComponent({
		name: 'ConfirmByNameDialog',
		components: {
			AlertDialog,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			Button,
			Input,
		},
		props: {
			open: { type: Boolean, default: false },
			title: { type: String, required: true },
			description: { type: String, default: '' },
			expectedName: { type: String, required: true },
			confirmLabel: { type: String, default: '' },
			busy: { type: Boolean, default: false },
		},
		emits: ['update:open', 'confirm'],
		setup(props, { emit }) {
			const typed = ref('');
			const matches = computed(() =>
				isConfirmNameMatch(typed.value, props.expectedName),
			);
			const canConfirm = computed(() => matches.value && !props.busy);

			watch(
				() => props.open,
				(isOpen) => {
					if (!isOpen) typed.value = '';
				},
			);

			const confirm = () => {
				if (canConfirm.value) emit('confirm', typed.value);
			};

			return { typed, canConfirm, confirm };
		},
	});
</script>

<template>
	<AlertDialog :open="open" @update:open="$emit('update:open', $event)">
		<AlertDialogContent>
			<AlertDialogHeader>
				<AlertDialogTitle>{{ title }}</AlertDialogTitle>
				<AlertDialogDescription>{{ description }}</AlertDialogDescription>
			</AlertDialogHeader>
			<i18n-t
				keypath="general.confirmByNameDialog.typeToConfirm"
				tag="p"
				class="text-sm"
			>
				<template #name>
					<strong>{{ expectedName }}</strong>
				</template>
			</i18n-t>
			<Input
				v-model="typed"
				autocomplete="off"
				spellcheck="false"
				:aria-label="$t('general.confirmByNameDialog.nameLabel')"
				@keydown.enter.prevent="confirm"
			/>
			<AlertDialogFooter>
				<AlertDialogCancel :disabled="busy">{{
					$t('common.actions.cancel')
				}}</AlertDialogCancel>
				<Button variant="destructive" :disabled="!canConfirm" @click="confirm">
					{{ confirmLabel || $t('common.actions.confirm') }}
				</Button>
			</AlertDialogFooter>
		</AlertDialogContent>
	</AlertDialog>
</template>
