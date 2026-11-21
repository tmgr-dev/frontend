<template>
	<div class="manager app-shell">
		<aside class="side">
			<div class="top">
				<b>{{ $t('manager.list.label') }}</b>
				<button
					type="button"
					class="btn pri sm"
					@click="store.openNewSession()"
				>
					{{ $t('manager.list.newSession') }}
				</button>
				<button
					type="button"
					class="ibd"
					:aria-label="$t('manager.trust.devices.open')"
					:title="$t('manager.trust.devices.open')"
					@click="trust.openPanel()"
				>
					<Icon name="gear" />
				</button>
			</div>
			<button type="button" class="sidesrch" @click="store.openSearch()">
				<span>{{ $t('manager.search.scopeAll') }}</span>
				<kbd>&#8984;K</kbd>
			</button>
			<ChatList
				:rooms="store.rooms"
				:active-key="store.activeKey ?? undefined"
				:unread="store.unreadByRoom"
				@select="store.select"
			/>
		</aside>
		<section class="main">
			<ChatHeader
				v-if="activeInfo"
				:info="activeInfo"
				@search="store.openSearch(store.activeKey ?? undefined)"
				@switch="store.openSwitch()"
			/>
			<SearchPanel
				v-if="store.search.open"
				:query="store.search.text"
				:scope="store.search.scopeKey ? 'chat' : 'all'"
				:chat-label="activeInfo?.title ?? ''"
				:hits="store.search.hits"
				:room-titles="roomTitles"
				@update:query="store.runSearch"
				@update:scope="onScope"
				@select="onHit"
			/>
			<template v-else>
				<MessageList
					ref="list"
					:messages="messages"
					:body-html="bodyHtml"
					:empty-hint="
						activeInfo?.room.kind === 'session'
							? $t('manager.messages.emptySession')
							: undefined
					"
					@reply="store.setReply"
					@copy="copy"
					@older="store.loadOlder"
				>
					<template #decision="{ message }">
						<template v-if="message.decision?.card_id">
							<DecisionCard
								v-if="activeInfo?.local !== false"
								:card-id="message.decision.card_id"
							/>
							<p v-else class="mgr-remote-decision">
								{{ $t('manager.decision.otherMachine') }}
							</p>
						</template>
					</template>
				</MessageList>
				<Composer
					v-model="store.draft.text"
					:reply-to="store.replyTo ?? undefined"
					:attachments="store.draft.attachments"
					@update:model-value="store.onInput"
					@send="store.send"
					@clear-reply="store.setReply(null)"
					@remove-attachment="store.removeAttachment"
					@attach="pick"
					@drop="addFiles"
					@keydown="onKey"
				>
					<template #popover
						><SlashMenu
							v-if="store.slash.open"
							:commands="store.slash.items"
							:active-index="store.slash.index"
							@select="store.pickSlash"
					/></template>
				</Composer>
			</template>
			<p v-if="store.error" class="mgr-error" role="alert">
				{{ store.error }}
			</p>
		</section>
		<DetailPanel
			:tab="store.panel.tab"
			:rooms="store.rooms"
			:info="activeInfo"
			:active-key="store.activeKey ?? undefined"
			:messages="messages"
			:audit="store.panel.audit"
			:audit-error="store.panel.auditError"
			:trust="trust.trust"
			@tab="store.setTab"
			@select="store.selectFromHierarchy"
			@devices="trust.openPanel()"
		/>
		<input ref="file" type="file" multiple hidden @change="onFiles" />
		<NewSessionDialog
			v-if="store.newSession.open"
			:form="store.newSession.form"
			:machines="store.machines"
			:busy="store.newSession.busy"
			:error="store.newSession.error"
			@submit="store.submitNewSession()"
			@cancel="store.closeNewSession()"
		/>
		<SwitchAgentDialog
			v-else-if="store.switcher.open"
			:agent="store.switcher.agent"
			:model="store.switcher.model"
			:models="AGENT_MODELS[store.switcher.agent]"
			:current="activeInfo?.agent"
			:busy="store.switcher.busy"
			:error="store.switcher.error"
			@pick="store.pickSwitchAgent"
			@update:model="store.switcher.model = $event"
			@submit="store.submitSwitch()"
			@cancel="store.closeSwitch()"
		/>
		<JoinConfirmDialog
			v-else-if="trust.current?.type === 'join'"
			:label="trust.current.join.label"
			:kind="trust.current.join.kind"
			:fp="trust.current.join.fp"
			:busy="trust.busy"
			:error="trust.error"
			@approve="trust.decideJoin(true)"
			@reject="trust.decideJoin(false)"
		/>
		<RevokeConfirmDialog
			v-else-if="trust.current?.type === 'revoke'"
			:revoker-label="trust.current.revoke.revoker_label"
			:revoker-fp="trust.current.revoke.revoker_fp"
			:target-label="trust.current.revoke.target_label"
			:target-fp="trust.current.revoke.target_fp"
			:busy="trust.busy"
			:error="trust.error"
			@approve="trust.decideRevoke(true)"
			@reject="trust.decideRevoke(false)"
			@dismiss="trust.deferRevoke()"
		/>
		<RevokeConfirmDialog
			v-else-if="revokeDevice"
			:target-label="revokeDevice.label"
			:target-fp="revokeDevice.fp"
			:busy="trust.busy"
			:error="trust.error"
			@approve="trust.confirmRevoke()"
			@reject="trust.cancelRevoke()"
			@dismiss="trust.cancelRevoke()"
		/>
		<ConfirmKindDialog
			v-else-if="confirmKindDevice && trust.confirmKindTarget"
			:label="confirmKindDevice.label"
			:fp="confirmKindDevice.fp"
			:kind="trust.confirmKindTarget.kind"
			:busy="trust.busy"
			:error="trust.error"
			@approve="trust.submitConfirmKind()"
			@cancel="trust.cancelConfirmKind()"
		/>
		<ResetIdentityDialog
			v-else-if="trust.resetOpen"
			v-model="trust.resetText"
			:busy="trust.busy"
			:error="trust.error"
			@confirm="trust.resetIdentity()"
			@cancel="trust.cancelReset()"
		/>
		<PairDeviceDialog
			v-else-if="trust.pair.mode"
			v-model="trust.pair.text"
			:mode="trust.pair.mode"
			:code="trust.pair.code"
			:qr="trust.pair.qr"
			:seconds-left="trust.pair.secondsLeft"
			:expired="trust.pair.expired"
			:busy="trust.busy"
			:error="trust.error"
			@submit="trust.submitCode()"
			@renew="trust.openAddDevice()"
			@close="trust.closePair()"
		/>
		<DevicesPanel
			v-else-if="trust.panelOpen"
			:devices="trust.trust?.devices ?? []"
			:key-mismatch="trust.trust?.key_mismatch ?? []"
			:alerts="trust.trust?.alerts ?? []"
			:seq-gaps="trust.trust?.seq_gaps ?? []"
			:can-join="trust.trust?.can_join_with_code ?? false"
			:joining="trust.trust?.joining ?? false"
			:busy="trust.busy"
			:error="trust.error"
			@revoke="trust.revoke"
			@confirm-kind="trust.askConfirmKind"
			@reset="trust.openReset()"
			@add-device="trust.openAddDevice()"
			@enter-code="trust.openEnterCode()"
			@close="trust.closePanel()"
		/>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import { onManagerResync } from './api';
	import ChatHeader from './components/ChatHeader.vue';
	import ChatList from './components/ChatList.vue';
	import Composer from './components/Composer.vue';
	import DecisionCard from './components/DecisionCard.vue';
	import DetailPanel from './components/DetailPanel.vue';
	import Icon from './components/Icon.vue';
	import MessageList from './components/MessageList.vue';
	import NewSessionDialog from './components/NewSessionDialog.vue';
	import SearchPanel from './components/SearchPanel.vue';
	import SlashMenu from './components/SlashMenu.vue';
	import SwitchAgentDialog from './components/SwitchAgentDialog.vue';
	import ConfirmKindDialog from './components/trust/ConfirmKindDialog.vue';
	import DevicesPanel from './components/trust/DevicesPanel.vue';
	import JoinConfirmDialog from './components/trust/JoinConfirmDialog.vue';
	import PairDeviceDialog from './components/trust/PairDeviceDialog.vue';
	import ResetIdentityDialog from './components/trust/ResetIdentityDialog.vue';
	import RevokeConfirmDialog from './components/trust/RevokeConfirmDialog.vue';
	import { renderBody } from './format';
	import { AGENT_MODELS, MAX_FILE_BYTES, useManagerStore } from './store';
	import { useTrustStore } from './trustStore';
	import { roomKey } from './types';

	const sha256Hex = async (file: File): Promise<string> => {
		const digest = await crypto.subtle.digest(
			'SHA-256',
			await file.arrayBuffer(),
		);
		return Array.from(new Uint8Array(digest), (b) =>
			b.toString(16).padStart(2, '0'),
		).join('');
	};

	export default defineComponent({
		name: 'ManagerApp',
		components: {
			ChatHeader,
			ChatList,
			Composer,
			ConfirmKindDialog,
			DecisionCard,
			DetailPanel,
			DevicesPanel,
			PairDeviceDialog,
			Icon,
			JoinConfirmDialog,
			MessageList,
			NewSessionDialog,
			ResetIdentityDialog,
			RevokeConfirmDialog,
			SearchPanel,
			SlashMenu,
			SwitchAgentDialog,
		},
		setup() {
			return {
				store: useManagerStore(),
				trust: useTrustStore(),
				AGENT_MODELS,
			};
		},
		computed: {
			revokeDevice() {
				return (
					this.trust.trust?.devices.find(
						(d) => d.id === this.trust.revokeTarget,
					) ?? null
				);
			},
			confirmKindDevice() {
				return (
					this.trust.trust?.devices.find(
						(d) => d.id === this.trust.confirmKindTarget?.id,
					) ?? null
				);
			},
			activeInfo() {
				return this.store.rooms.find(
					(r) => roomKey(r.room) === this.store.activeKey,
				);
			},
			messages() {
				return this.store.activeKey
					? (this.store.messagesByRoom[this.store.activeKey] ?? [])
					: [];
			},
			bodyHtml(): Record<string, string> {
				return Object.fromEntries(
					this.messages.map((m) => [m.id, renderBody(m.body_md)]),
				);
			},
			roomTitles(): Record<string, string> {
				return Object.fromEntries(
					this.store.rooms.map((r) => [roomKey(r.room), r.title]),
				);
			},
		},
		data() {
			return {
				unlistenResync: null as (() => void) | null,
				stopWatch: null as (() => void) | null,
			};
		},
		mounted() {
			void this.store.init();
			void this.trust.init();
			window.addEventListener('keydown', this.onGlobalKey);
			window.addEventListener('focus', this.onWindowFocus);
			this.stopWatch = this.store.watchRooms(5000);
			void onManagerResync(() => void this.store.refreshActive()).then(
				(off) => {
					this.unlistenResync = off;
				},
			);
		},
		beforeUnmount() {
			this.unlistenResync?.();
			this.stopWatch?.();
			window.removeEventListener('keydown', this.onGlobalKey);
			window.removeEventListener('focus', this.onWindowFocus);
			this.trust.dispose();
		},
		methods: {
			onWindowFocus() {
				void this.store.loadRooms().catch(() => undefined);
			},
			onGlobalKey(e: KeyboardEvent) {
				if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
					e.preventDefault();
					this.store.openSearch();
				} else if (e.key === 'Escape' && this.store.search.open) {
					this.store.closeSearch();
				}
			},
			onKey(e: KeyboardEvent) {
				if (!this.store.slash.open) return;
				if (e.key === 'ArrowDown') {
					e.preventDefault();
					this.store.moveSlash(1);
				} else if (e.key === 'ArrowUp') {
					e.preventDefault();
					this.store.moveSlash(-1);
				} else if (e.key === 'Tab' || e.key === 'Enter') {
					e.preventDefault();
					this.store.pickSlash();
				} else if (e.key === 'Escape') {
					this.store.closeSlash();
				}
			},
			onScope(scope: 'chat' | 'all') {
				this.store.search.scopeKey =
					scope === 'chat' ? this.store.activeKey : null;
				void this.store.runSearch(this.store.search.text);
			},
			async onHit(hit: Parameters<typeof this.store.jumpToHit>[0]) {
				this.store.closeSearch();
				await this.store.jumpToHit(hit);
			},
			copy(text: string) {
				void navigator.clipboard?.writeText(text);
			},
			pick() {
				(this.$refs.file as HTMLInputElement).click();
			},
			onFiles(e: Event) {
				const input = e.target as HTMLInputElement;
				void this.addFiles(Array.from(input.files ?? []));
				input.value = '';
			},
			async addFiles(files: File[]) {
				for (const file of files) {
					if (file.size > MAX_FILE_BYTES) {
						this.store.error = `${file.name} is larger than 25 MiB`;
						continue;
					}
					this.store.addAttachment({
						name: file.name,
						type: file.type || 'application/octet-stream',
						size: file.size,
						sha256: await sha256Hex(file),
					});
				}
			},
		},
	});
</script>

<style src="./manager.css"></style>
