<template>
	<Dialog
		v-if="available"
		:open="open"
		@update:open="(value) => $emit('update:open', value)"
	>
		<DialogContent data-testid="bypass-consent">
			<DialogHeader>
				<DialogTitle>Run agents without permission prompts</DialogTitle>
				<DialogDescription>
					In bypass mode an agent can change files, run commands and use the
					network on this computer without asking you first. A mistake or a
					malicious instruction in a task cannot be stopped by a prompt.
				</DialogDescription>
			</DialogHeader>
			<label class="flex flex-col gap-1.5 text-sm">
				Type {{ BYPASS_PHRASE }} to continue
				<Input v-model="typed" data-testid="bypass-phrase" autocomplete="off" />
			</label>
			<DialogFooter>
				<Button variant="outline" @click="$emit('update:open', false)">
					Cancel
				</Button>
				<Button
					variant="destructive"
					:disabled="!isBypassPhrase(typed)"
					data-testid="bypass-confirm"
					@click="$emit('confirm')"
				>
					Enable bypass mode
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
