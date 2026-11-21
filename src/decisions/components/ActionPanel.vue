<template>
	<section class="action" data-action-panel>
		<h2 class="action-h">{{ $t(header.key, header.params) }}</h2>

		<div v-for="row in rows" :key="row.field" class="arow">
			<div class="alabel">
				<span>{{ $t('decisions.action.field.' + row.field) }}</span>
				<template v-if="row.editable && open">
					<span v-if="isEdited(row.field)" class="aedited" data-edited>{{
						$t('decisions.action.edited')
					}}</span>
					<button
						v-if="isEdited(row.field)"
						type="button"
						class="areset"
						data-reset
						@click="reset(row.field)"
					>
						{{ $t('decisions.action.reset') }}
					</button>
				</template>
			</div>
			<template v-if="row.editable && open">
				<textarea
					v-if="row.multiline"
					v-model="values[row.field]"
					class="ainput"
					rows="4"
					:data-field="row.field"
				></textarea>
				<input
					v-else
					v-model="values[row.field]"
					class="ainput"
					type="text"
					:data-field="row.field"
				/>
			</template>
			<div v-else class="avalue" :class="{ mono: row.mono }">
				{{ row.value }}
			</div>
		</div>

		<div v-if="commits.length" class="arow">
			<div class="alabel">{{ $t('decisions.action.field.commits') }}</div>
			<ul class="acommits">
				<li v-for="commit in commits" :key="commit.sha">
					<code>{{ shortSha(commit.sha) }}</code> {{ commit.subject }}
				</li>
			</ul>
		</div>

		<div v-if="action.kind === 'pr'" class="notice" data-pr-note>
			{{ $t('decisions.action.prNote') }}
		</div>

		<div v-if="status" class="arun" :class="'s-' + status.state" data-run>
			<div v-if="status.state === 'executing'" class="arun-t">
				<span class="aspin" aria-hidden="true"></span
				>{{ $t('decisions.action.run.executing') }}
			</div>
			<div v-else class="arun-t">
				{{ $t('decisions.action.run.' + status.state) }}
			</div>
			<div v-if="status.url" class="arun-l">
				<button
					v-if="isHttpUrl(status.url)"
					type="button"
					class="chip"
					:title="status.url"
					@click="$emit('open-link', status.url)"
				>
					{{ status.url }}
				</button>
				<span v-else class="chip">{{ status.url }}</span>
				<code v-if="status.sha">{{ shortSha(status.sha) }}</code>
			</div>
			<div v-else-if="status.sha" class="arun-l">
				<code>{{ status.sha }}</code>
			</div>
			<div v-if="status.error" class="arun-e">{{ status.error }}</div>
		</div>

		<div v-if="rejectReason" class="arun s-failed" data-rejected>
			<div class="arun-t">{{ $t('decisions.action.rejected') }}</div>
			<div class="arun-e">{{ rejectReason }}</div>
		</div>

		<div v-if="open" class="abar">
			<template v-if="!rejecting">
				<button type="button" class="btn pri" data-approve @click="approve">
					{{ $t('decisions.action.approve') }}
				</button>
				<button
					type="button"
					class="btn"
					data-reject-start
					@click="rejecting = true"
				>
					{{ $t('decisions.action.reject') }}
				</button>
			</template>
			<div v-else class="areject">
				<textarea
					v-model="reason"
					class="ainput"
					rows="2"
					data-reason
					:placeholder="$t('decisions.action.reasonPlaceholder')"
				></textarea>
				<div class="abar">
					<button
						type="button"
						class="btn"
						data-reject
						:disabled="!reason.trim()"
						@click="reject"
					>
						{{ $t('decisions.action.reject') }}
					</button>
					<button type="button" class="btn" @click="cancelReject">
						{{ $t('decisions.action.cancel') }}
					</button>
				</div>
			</div>
		</div>
	</section>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import {
		actionHeader,
		approveDraft,
		fieldValue,
		isActionOpen,
		rejectDraft,
		runStatus,
		shortSha,
		type ActionHeader,
		type RunStatus,
	} from '../actionPanel';
	import { isHttpUrl } from '../format';
	import type { ActionCommit, CardAction, CardView } from '../types';

	interface Row {
		field: string;
		value: string;
		editable: boolean;
		multiline: boolean;
		mono: boolean;
	}

	const FIELDS: Record<CardAction['kind'], string[]> = {
		comment: ['text'],
		task: ['title', 'description', 'workspace_id', 'category_id', 'status_id'],
		pr: ['title', 'body', 'head_sha', 'base_sha'],
		tag: ['name', 'target_sha'],
	};
	const MULTILINE = new Set(['text', 'description', 'body']);
	const MONO = new Set(['head_sha', 'base_sha', 'target_sha']);

	export default defineComponent({
		name: 'ActionPanel',
		props: {
			card: { type: Object as PropType<CardView>, required: true },
		},
		emits: ['answer', 'open-link'],
		data() {
			const action = this.card.action as CardAction;
			const values: Record<string, string> = {};
			for (const field of action.editable) {
				values[field] = fieldValue(action, field);
			}
			return { values, rejecting: false, reason: '' };
		},
		computed: {
			action(): CardAction {
				return this.card.action as CardAction;
			},
			open(): boolean {
				return isActionOpen(this.card);
			},
			header(): ActionHeader {
				return actionHeader(this.action);
			},
			status(): RunStatus | null {
				return runStatus(this.action);
			},
			commits(): ActionCommit[] {
				return this.action.payload.commits ?? [];
			},
			rejectReason(): string {
				const answer = this.card.answer;
				return answer?.choice === 'reject'
					? (answer.note ?? answer.free_text ?? '')
					: '';
			},
			rows(): Row[] {
				const payload = this.action.payload as Record<string, unknown>;
				return FIELDS[this.action.kind]
					.filter(
						(field) =>
							this.action.editable.includes(field) ||
							(payload[field] !== undefined && payload[field] !== null),
					)
					.map((field) => ({
						field,
						value: String(payload[field] ?? ''),
						editable: this.action.editable.includes(field),
						multiline: MULTILINE.has(field),
						mono: MONO.has(field),
					}));
			},
		},
		methods: {
			shortSha,
			isHttpUrl,
			isEdited(field: string): boolean {
				return this.values[field] !== fieldValue(this.action, field);
			},
			reset(field: string) {
				this.values[field] = fieldValue(this.action, field);
			},
			approve() {
				this.$emit('answer', approveDraft(this.action, this.values));
			},
			reject() {
				const draft = rejectDraft(this.reason);
				if (draft) this.$emit('answer', draft);
			},
			cancelReject() {
				this.rejecting = false;
				this.reason = '';
			},
		},
	});
</script>
