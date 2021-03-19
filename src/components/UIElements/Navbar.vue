<template>
  <div class="ml-auto md:hidden absolute z-30 right-0 p-4">
		<div class="nav-toggle-menu" :class="{'active': menuIsActive}" @click="$store.getters.slideout.toggle()">
			<div class="bar1" :class="$color('burgerIcon')"></div>
			<div class="bar2" :class="$color('burgerIcon')"></div>
			<div class="bar3" :class="$color('burgerIcon')"></div>
		</div>
  </div>
	<nav :class="`${$color('menuBg')} shadow md:block tc-hidden`" role="navigation">
		<div class="container mx-auto p-4 flex flex-wrap items-center md:flex-no-wrap">
			<div :class="`w-full md:w-auto md:flex-grow md:flex md:items-center ${isHidden ? 'tc-hidden' : ''}`">
				<navbar-menu />
				<span :class="`${$color('navTextUser')}-500`" class="flex justify-between items-center ml-auto mr-4" :key="rerenderSwitcher">
					<day-night-switch v-model="switchOn"/>
				</span>
				<account-dropdown />
			</div>
		</div>
	</nav>
</template>

<script>
	import DayNightSwitch from './DayNightSwitch'
	import AccountDropdown from './AccountDropdown'
	import NavbarMenu from "src/components/UIElements/NavbarMenu";

	export default {
		name: 'Navbar',
		props: {
			menuIsActive: {
				type: Boolean,
				required: false,
				default: false
			},
			menuPosition: {
				type: Number,
				required: false,
				default: 0
			}
		},
		components: {
			NavbarMenu,
			AccountDropdown,
			DayNightSwitch
		},
		data: () => ({
			isHidden: true,
			rerenderSwitcher: 0,
			links: [
				{ id: 1, name: 'Current tasks', path: '/' },
				{ id: 1, name: 'tc-hidden', path: '/tc-hidden' },
				{ id: 1, name: 'Archive', path: '/acrhive' },
				{ id: 1, name: 'Categories', path: '/projects-categories' },
			],
			wheelEvent: 'onwheel' in document.createElement('div') ? 'wheel' : 'mousewheel',
			supportsPassive: false
		}),
		computed: {
			/*bar1Transform () {
				const percentPos = Math.round(this.menuPosition/255*100)
				const rotate = Math.round(45 * (1 - percentPos/100))
				return {
					transform: `rotate(${-rotate+45}deg)`
				}
			},
			bar3Transform () {
				const percentPos = Math.round(this.menuPosition/255*100)
				const rotate = Math.round(45 * (1 - percentPos/100))
				return {
					transform: `rotate(${rotate-45}deg)`
				}
			},*/
			wheelOpt () {
				return this.supportsPassive ? { passive: false } : false
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
		  isHidden(newVal) {
		    if (newVal) {
          return this.enableScroll()
        }
		    return this.disableScroll()
      }
    },
    methods: {
      preventDefault(e) {
        e.preventDefault();
      },
      preventDefaultForScrollKeys(e) {
        if (keys[e.keyCode]) {
          this.preventDefault(e);
          return false;
        }
      },
      disableScroll() {
        window.addEventListener('DOMMouseScroll', this.preventDefault, false); // older FF
        window.addEventListener(this.wheelEvent, this.preventDefault, this.wheelOpt); // modern desktop
        window.addEventListener('touchmove', this.preventDefault, this.wheelOpt); // mobile
        window.addEventListener('keydown', this.preventDefaultForScrollKeys, false);
      },
      enableScroll() {
        window.removeEventListener('DOMMouseScroll', this.preventDefault, false);
        window.removeEventListener(this.wheelEvent, this.preventDefault, this.wheelOpt);
        window.removeEventListener('touchmove', this.preventDefault, this.wheelOpt);
        window.removeEventListener('keydown', this.preventDefaultForScrollKeys, false);
      }
    },
		created() {
			window.addEventListener("test", null, Object.defineProperty({}, 'passive', {
				get: () => this.supportsPassive = true
			}));
		}
	}
</script>

<style>
.fade-kiosk-list-enter-active,
.fade-kiosk-list-leave-active {
  transition: all 0.9s ease-in-out;
}

.mobile-navbar-menu {
  /*transform: translateX(-50%);*/
}

.fade-kiosk-list-enter {
  transform: translateX(100%);
}

.fade-kiosk-list-leave-to {
  transform: translateX(-100%);
}
</style>
