<template>
	<Dialog
		v-if="available"
		:open="open"
		@update:open="(value: boolean) => emit('update:open', value)"
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
				<Button variant="outline" @click="emit('update:open', false)">
					Cancel
				</Button>
				<Button
					variant="destructive"
					:disabled="!isBypassPhrase(typed)"
					data-testid="bypass-confirm"
					@click="emit('confirm')"
				>
					Enable bypass mode
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script setup lang="ts">
	import { computed, ref, watch } from 'vue';
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
	import { BYPASS_PHRASE, bypassAvailable, isBypassPhrase } from '@/runner/bypass';
	import { useRunner } from '@/runner/store';

	const props = defineProps<{ open: boolean }>();
	const emit = defineEmits<{
		(e: 'update:open', value: boolean): void;
		(e: 'confirm'): void;
	}>();

	const runner = useRunner();
	const available = computed(() => bypassAvailable(runner.status.value));
	const typed = ref('');
	watch(
		() => props.open,
		() => (typed.value = ''),
	);
</script>
