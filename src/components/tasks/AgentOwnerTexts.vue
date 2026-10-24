<template>
	<div class="flex flex-col gap-2" data-testid="agent-owner-texts">
		<details
			v-if="personaBlock"
			class="rounded border border-line p-2"
			data-testid="cloud-confirm-persona"
		>
			<summary
				class="cursor-pointer text-2xs font-bold uppercase text-ink-subtle"
			>
				Persona instructions (<template
					v-for="(piece, i) in personaBlock.name"
					:key="i"
					><mark v-if="piece.flagged" class="bg-amber-300 text-black">{{
						piece.text
					}}</mark
					><template v-else>{{ piece.text }}</template></template
				>)
				<span v-if="personaNote" class="font-normal normal-case">{{
					personaNote
				}}</span>
			</summary>
			<pre
				class="mt-1 max-h-48 overflow-y-auto whitespace-pre-wrap break-words font-mono text-xs"
			><template v-for="(piece, i) in personaBlock.pieces" :key="i"><mark v-if="piece.flagged" class="bg-amber-300 text-black">{{ piece.text }}</mark><template v-else>{{ piece.text }}</template></template></pre>
		</details>
		<details
			v-if="pageBlocks.length"
			class="rounded border border-line p-2"
			data-testid="cloud-confirm-rules"
		>
			<summary
				class="cursor-pointer text-2xs font-bold uppercase text-ink-subtle"
			>
				Workspace rules
				<span v-if="rulesNote" class="font-normal normal-case">{{
					rulesNote
				}}</span>
			</summary>
			<div
				v-for="page in pageBlocks"
				:key="page.id"
				class="mt-1"
				:data-testid="`cloud-confirm-page-${page.id}`"
			>
				<div class="text-xs font-bold">
					<template v-for="(piece, i) in page.title" :key="i"
						><mark v-if="piece.flagged" class="bg-amber-300 text-black">{{
							piece.text
						}}</mark
						><template v-else>{{ piece.text }}</template></template
					>
				</div>
				<pre
					class="max-h-48 overflow-y-auto whitespace-pre-wrap break-words font-mono text-xs"
				><template v-if="!page.body.length">(empty)</template><template v-for="(piece, i) in page.body" :key="i"><mark v-if="piece.flagged" class="bg-amber-300 text-black">{{ piece.text }}</mark><template v-else>{{ piece.text }}</template></template></pre>
			</div>
		</details>
	</div>
</template>

<script lang="ts">
	import type {
		ApprovalContext,
		ApprovalPersona,
	} from '@/actions/tmgr/agentRuns';
	import { splitInvisible } from '@/utils/agentApprovalPolicy';
	import { computed, defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'AgentOwnerTexts',
		props: {
			persona: {
				type: Object as PropType<ApprovalPersona | null>,
				default: null,
			},
			context: {
				type: Object as PropType<ApprovalContext | null>,
				default: null,
			},
			personaNote: { type: String, default: '' },
			rulesNote: { type: String, default: '' },
		},
		setup(props) {
			const personaBlock = computed(() =>
				props.persona
					? {
							name: splitInvisible(props.persona.name),
							pieces: splitInvisible(props.persona.instructions),
						}
					: null,
			);
			const pageBlocks = computed(() =>
				(props.context?.pages ?? []).map((page) => ({
					id: page.id,
					title: splitInvisible(page.title),
					body: splitInvisible(page.body),
				})),
			);
			return { personaBlock, pageBlocks };
		},
	});
</script>
