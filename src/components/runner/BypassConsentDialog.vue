<template>
	<Dialog
		v-if="available"
		:open="open"
		@update:open="(value) => $emit('update:open', value)"
	>
		<DialogContent data-testid="bypass-consent">
			<DialogHeader>
				<DialogTitle>{{ $t('desktop.runner.bypass.title') }}</DialogTitle>
				<DialogDescription>
					{{ $t('desktop.runner.bypass.description') }}
				</DialogDescription>
			</DialogHeader>
			<label class="flex flex-col gap-1.5 text-sm">
				{{ $t('desktop.runner.bypass.type', { phrase: BYPASS_PHRASE }) }}
				<Input v-model="typed" data-testid="bypass-phrase" autocomplete="off" />
			</label>
			<DialogFooter>
				<Button variant="outline" @click="$emit('update:open', false)">
					{{ $t('common.actions.cancel') }}
				</Button>
				<Button
					variant="destructive"
					:disabled="!isBypassPhrase(typed)"
					data-testid="bypass-confirm"
					@click="$emit('confirm')"
				>
					{{ $t('desktop.runner.bypass.enable') }}
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
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
	import { Input } from '@/components/ui/input';
	import {
		BYPASS_PHRASE,
		bypassAvailable,
		isBypassPhrase,
	} from '@/runner/bypass';
	import { useRunner } from '@/runner/store';
	import { computed, defineComponent, ref, watch } from 'vue';

	export default defineComponent({
		name: 'BypassConsentDialog',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			Input,
		},
		props: { open: { type: Boolean, default: false } },
		emits: ['update:open', 'confirm'],
		setup(props) {
			const runner = useRunner();
			const available = computed(() => bypassAvailable(runner.status.value));
			const typed = ref('');
			watch(
				() => props.open,
				() => (typed.value = ''),
			);
			return { available, typed, BYPASS_PHRASE, isBypassPhrase };
		},
	});
</script>
