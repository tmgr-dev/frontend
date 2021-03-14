<template>
	<div id="q-app" :class="$color('textMain')">
		<Slideout menu="#menu" panel="#panel" :toggle-selectors="['a.toggle-button']" @on-open="open">
			<div id="menu" class="overflow-y-hidden" style="overflow-y: hidden;">
				<div
					:class="`z-20 pl-5`"
				>
					<a href="#" class="absolute right-0 p-2" @click.prevent="$store.getters.slideout.toggle()">
						<svg fill="currentColor" viewBox="0 0 20 20" class="w-6 h-6">
							<path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd"></path>
						</svg>
					</a>
					<navbar-menu />
					<account-dropdown />
					<span :class="`${$color('navTextUser')}-500`" class="absolute bottom-0 pb-10" :key="rerenderSwitcher">
            <day-night-switch v-model="switchOn"/>
          </span>
				</div>
			</div>
			<div id="panel" :class="`${$color('bgBody')}`">
				<transition name="fade" mode="out-in">
					<Navbar v-if="$route.meta.navbarHidden" />
				</transition>
				<router-view :key="$route.path" v-slot="{ Component }">
					<transition
						:name="transitionName"
						mode="out-in"
						@before-leave="beforeLeave"
						@enter="enter"
						@after-enter="afterEnter">
						<component v-show="showComponent" :is="Component"></component>
					</transition>
				</router-view>
			</div>
		</Slideout>
		<div :class="`fixed right-0 bottom-0 ml-5 mb-10 mr-2 z-10`">
				<span
					v-for="task in activeTasks"
					class="mb-5 inline-block"
				>
					<transition name="fade" mode="out-in">
						<span
							:class="`relative inline-flex rounded-md shadow-sm p-2 mr-5 ${$color('activeTaskReminderBg')}`"
							v-if="task.id !== $store.getters.currentOpenedTaskId"
						>
							<span class="flex absolute h-5 w-5 top-0 left-0 -mt-2 -ml-2">
								<span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"></span>
								<span class="relative inline-flex rounded-full h-5 w-5 bg-green-500"></span>
							</span>
							<div :class="$color('textMain')">
								<router-link :to="`/${task.id}/edit`">{{ task.title }}</router-link>
							</div>
						</span>
					</transition>
				</span>
		</div>
	</div>
</template>

<script>
	import { defineComponent } from 'vue'

	import Navbar from "src/components/UIElements/Navbar";
	import NavbarMenu from "src/components/UIElements/NavbarMenu";
	import './assets/styles/index.scss';

	const DEFAULT_TRANSITION = 'fade'

	export default defineComponent({
		name: 'App',
		components: {
			Navbar,
			NavbarMenu
		},
		data() {
			return {
				prevHeight: 0,
				transitionName: DEFAULT_TRANSITION,
				showComponent: true,
				activeTasks: []
			};
		},
		computed: {
			navbarHidden () {
				return this.$route.name !== 'Index'
			},
			switchOn: {
				get () {
					return this.$store.getters.colorScheme === 'dark'
				},
				set (newValue) {
					this.$store.commit('colorScheme', newValue ? 'dark' : 'default')
				}
			}
		},
		watch: {
			'$route.path' () {
				this.showComponent = false
				setTimeout(() => this.showComponent = true, 100)
			}
		},
		methods: {
			beforeLeave(element) {
				this.prevHeight = getComputedStyle(element).height;
			},
			enter(element) {
				const {height} = getComputedStyle(element);

				element.style.height = this.prevHeight;

				setTimeout(() => {
					element.style.height = height;
				});
			},
			afterEnter(element) {
				element.style.height = 'auto';
			},
			async loadActiveTasks() {
				if (!this.$store.getters.user) {
					return
				}
				const {data: {data}} = await this.$axios.get('/tasks/runned')
				this.activeTasks = data
			}
		},
		async created() {
			this.$store.dispatch('loadUserSettings')

			this.$router.beforeEach((to, from, next) => {
				this.$store.commit('currentOpenedTaskId', null)
				this.loadActiveTasks()
				let transitionName = to.meta.transitionName || from.meta.transitionName;

				if (transitionName === 'slide') {
					const toDepth = to.path.split('/').length;
					const fromDepth = from.path.split('/').length;
					transitionName = toDepth < fromDepth ? 'slide-right' : 'slide-left';
				}

				this.transitionName = transitionName || DEFAULT_TRANSITION;

				next();
			});

			if (!this.$store.getters.user) {
				return;
			}
			this.$store.getters.pusherBeamsClient.getUserId().then((userId) => {
				if (!userId) {
					return this.$store.commit('pusherBeamsUserId', userId);
				}
				userId = this.$store.getters.user.id.toString()
				this.$store.getters.pusherBeamsClient.start()
					.then(() => {
						this.$store.getters.pusherBeamsClient.setUserId(userId, this.$store.getters.pusherTokenProvider).then(() => {
							this.$store.commit('pusherBeamsUserId', userId)
						})
					});
			})
			this.loadActiveTasks()
		}
	})
</script>
