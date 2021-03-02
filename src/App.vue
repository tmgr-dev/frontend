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
			<div id="panel" :class="$color('bgBody')">
				<transition
					name="fade"
					mode="out-in"
				>
					<Navbar v-if="$route.meta.navbarHidden" />
				</transition>
				<router-view :key="$route.path" v-slot="{ Component }">
					<transition
						:name="transitionName"
						mode="out-in"
						@before-leave="beforeLeave"
						@enter="enter"
						@after-enter="afterEnter"
					>
						<component v-show="showComponent" :is="Component"></component>
					</transition>
				</router-view>
			</div>
		</Slideout>
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
				showComponent: true
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
		},
		created() {
			this.$store.dispatch('loadUserSettings')

			this.$router.beforeEach((to, from, next) => {
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
			this.$store.getters.pusherBeamsClient.start()
				.then(() => this.$store.getters.pusherBeamsClient.setUserId(this.$store.getters.user.id.toString(), this.$store.getters.pusherTokenProvider));
		}
	})
</script>

<style>
	.text-main {
		color: #2c3e50;
	}

	.dark-text-main {
		color: #e2e2e2;
	}

	.dark-bg-body {
		background-color: #121212 !important;
	}

	.bg-body {
		background-color: #edf2f7;
	}

	.fade-enter-active,
	.fade-leave-active {
		transition-duration: 0.5s;
		transition-property: height, opacity;
		transition-timing-function: ease;
		overflow: hidden;
	}

	.fade-enter,
	.fade-leave-active {
		opacity: 0
	}

	.slide-left-enter-active,
	.slide-left-leave-active,
	.slide-right-enter-active,
	.slide-right-leave-active {
		transition-duration: 0.5s;
		transition-property: height, opacity, transform;
		transition-timing-function: cubic-bezier(0.55, 0, 0.1, 1);
		overflow: hidden;
	}

	.slide-left-enter,
	.slide-right-leave-active {
		opacity: 0;
		transform: translate(2em, 0);
	}

	.slide-left-leave-active,
	.slide-right-enter {
		opacity: 0;
		transform: translate(-2em, 0);
	}
</style>
