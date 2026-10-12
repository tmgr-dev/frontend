<template>
	<div>
		<Teleport to="body">
			<div
				ref="root"
				class="fixed inset-0 z-[55] overflow-y-auto bg-surface px-4 pt-6 dark:bg-gray-950"
				:class="consentOpen ? 'pb-72 sm:pb-12' : 'pb-12'"
				data-testid="welcome-setup"
			>
				<p class="text-xl font-bold tracking-tight text-brand">TMGR</p>
				<main class="mx-auto mt-10 max-w-3xl text-center sm:mt-16">
					<h1 class="text-2xl font-semibold text-ink sm:text-3xl">How do you want to start?</h1>
					<p class="mt-2 text-sm text-ink-subtle">You can do both later. Nothing here is permanent.</p>

					<div class="mt-8 grid grid-cols-1 gap-4 sm:grid-cols-2" role="group" aria-label="Start with">
						<IntentCard
							title="Just track my tasks"
							description="Tasks, a board and a timer. Ready in a minute."
							:icon="ListChecks"
							:selected="selected === 'human'"
							:disabled="busy"
							@select="choose('human')"
						/>
						<IntentCard
							title="Connect my AI agent"
							description="For Claude Code, Codex, Cursor or any MCP client."
							:icon="Bot"
							:selected="selected === 'agent'"
							:disabled="busy"
							@select="choose('agent')"
						/>
					</div>

					<p v-if="error" class="mt-4 text-sm text-red-600 dark:text-red-400" role="alert">
						{{ error }}
					</p>

					<div class="mt-6 flex flex-col items-center gap-1">
						<button
							type="button"
							class="inline-flex min-h-[44px] items-center text-sm text-ink-subtle underline hover:text-ink disabled:opacity-60"
							:disabled="busy"
							data-testid="intent-empty"
							@click="choose('human', false)"
						>
							Start with an empty workspace
						</button>
						<button
							type="button"
							class="inline-flex min-h-[44px] items-center text-sm font-medium text-brand underline hover:opacity-80 disabled:opacity-60"
							:disabled="busy"
							data-testid="intent-skip"
							@click="skip"
						>
							Skip, I'll look around
						</button>
					</div>

					<ol v-if="dots.length > 1" class="mt-10 flex items-center justify-center gap-2" aria-hidden="true">
						<li
							v-for="(dot, index) in dots"
							:key="dot"
							class="h-2 w-2 rounded-full"
							:class="index === 0 ? 'bg-brand' : 'bg-line dark:bg-gray-700'"
						/>
					</ol>
				</main>
			</div>
		</Teleport>
	</div>
</template>

<script lang="ts">
import { Bot, ListChecks } from 'lucide-vue-next';
import { computed, defineComponent, onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import { useStore } from 'vuex';
import IntentCard from '@/components/onboarding/IntentCard.vue';
import { steps } from '@/onboarding/flow';
import { useFocusedScreen } from '@/onboarding/useFocusedScreen';
import { useTours } from '@/tours/useTours';

const HUMAN_SEQUENCE = ['start.nav', 'start.first-task', 'time.timer'];

export default defineComponent({
	name: 'WelcomeSetup',
	components: { IntentCard },
	setup() {
		const router = useRouter();
		const store = useStore();
		const tours = useTours();
		const root = ref<HTMLElement | null>(null);
		const { consentOpen } = useFocusedScreen(root);
		const selected = ref<'human' | 'agent' | null>(null);
		const busy = ref(false);
		const error = ref('');

		const dots = computed(() => steps(selected.value ?? 'human'));

		const listPath = () => {
			const code = store.getters.currentWorkspace?.code;
			return code ? `/${code}/list` : '/';
		};

		const save = async (intent: string, seed?: boolean) => {
			busy.value = true;
			error.value = '';
			try {
				await tours.setIntent(intent, seed === undefined ? undefined : { seed });
				return true;
			} catch {
				error.value = 'Could not save your choice. Try again.';
				selected.value = null;
				busy.value = false;
				return false;
			}
		};

		onMounted(() => {
			if (store.getters.isLocalOnly) void router.replace(listPath());
		});

		const choose = async (intent: 'human' | 'agent', seed = true) => {
			if (busy.value) return;
			selected.value = intent;
			if (!(await save(intent, seed))) return;
			if (intent === 'agent') {
				await router.push({ name: 'WelcomeSetupAgent' });
				return;
			}
			await router.push(listPath());
			await tours.startSequence(HUMAN_SEQUENCE, 'first_run');
		};

		const skip = async () => {
			if (busy.value) return;
			if (!(await save('skipped'))) return;
			await router.push(listPath());
		};

		return { Bot, ListChecks, root, consentOpen, selected, busy, error, dots, choose, skip };
	},
});
</script>
