<script setup lang="ts">
import { cn } from '@/utils'
import { useEventListener, useMediaQuery, useVModel } from '@vueuse/core';
import { TooltipProvider } from 'radix-vue'
import { computed, type HTMLAttributes, onMounted, onUnmounted, type Ref, ref } from 'vue'
import { provideSidebarContext, SIDEBAR_COOKIE_MAX_AGE, SIDEBAR_COOKIE_NAME, SIDEBAR_KEYBOARD_SHORTCUT, SIDEBAR_WIDTH, SIDEBAR_WIDTH_ICON } from './utils'

const props = withDefaults(defineProps<{
	defaultOpen?: boolean
	open?: boolean
	class?: HTMLAttributes['class']
}>(), {
	defaultOpen: true,
	open: undefined,
})

const emits = defineEmits<{
	'update:open': [open: boolean]
}>()

const isMobile = useMediaQuery("(max-width: 768px)");
const openMobile = ref(false)

const open = useVModel(props, 'open', emits, {
	defaultValue: props.defaultOpen ?? false,
	passive: (props.open === undefined) as false,
}) as Ref<boolean>

function setOpen(value: boolean) {
	open.value = value // emits('update:open', value)

	// This sets the cookie to keep the sidebar state.
	document.cookie = `${SIDEBAR_COOKIE_NAME}=${open.value}; path=/; max-age=${SIDEBAR_COOKIE_MAX_AGE}`
}

function setOpenMobile(value: boolean) {
	openMobile.value = value
}

// Helper to toggle the sidebar.
function toggleSidebar() {
	return isMobile.value
		? setOpenMobile(!openMobile.value)
		: setOpen(!open.value);
}

useEventListener('keydown', (event: KeyboardEvent) => {
	if (event.key === SIDEBAR_KEYBOARD_SHORTCUT && (event.metaKey || event.ctrlKey)) {
		event.preventDefault()
		toggleSidebar()
	}
})

const SWIPE_THRESHOLD = 50
const EDGE_ZONE = 30
let touchStartX = 0
let touchStartY = 0
let isSwiping = false
let swipeDirection: 'open' | 'close' | null = null

function handleTouchStart(event: TouchEvent) {
	if (!isMobile.value) return

	const touch = event.touches[0]

	if (!openMobile.value && touch.clientX <= EDGE_ZONE) {
		touchStartX = touch.clientX
		touchStartY = touch.clientY
		isSwiping = true
		swipeDirection = 'open'
	} else if (openMobile.value) {
		touchStartX = touch.clientX
		touchStartY = touch.clientY
		isSwiping = true
		swipeDirection = 'close'
	}
}

function handleTouchMove(event: TouchEvent) {
	if (!isSwiping || !isMobile.value) return

	const touch = event.touches[0]
	const deltaX = touch.clientX - touchStartX
	const deltaY = Math.abs(touch.clientY - touchStartY)

	if (swipeDirection === 'open' && deltaX > SWIPE_THRESHOLD && deltaX > deltaY) {
		setOpenMobile(true)
		isSwiping = false
		swipeDirection = null
	} else if (swipeDirection === 'close' && deltaX < -SWIPE_THRESHOLD && Math.abs(deltaX) > deltaY) {
		setOpenMobile(false)
		isSwiping = false
		swipeDirection = null
	}
}

function handleTouchEnd() {
	isSwiping = false
	swipeDirection = null
}

onMounted(() => {
	document.addEventListener('touchstart', handleTouchStart, { passive: true })
	document.addEventListener('touchmove', handleTouchMove, { passive: true })
	document.addEventListener('touchend', handleTouchEnd, { passive: true })
})

onUnmounted(() => {
	document.removeEventListener('touchstart', handleTouchStart)
	document.removeEventListener('touchmove', handleTouchMove)
	document.removeEventListener('touchend', handleTouchEnd)
})

// We add a state so that we can do data-state="expanded" or "collapsed".
// This makes it easier to style the sidebar with Tailwind classes.
const state = computed(() => open.value ? 'expanded' : 'collapsed')

provideSidebarContext({
	state,
	open,
	setOpen,
	isMobile,
	openMobile,
	setOpenMobile,
	toggleSidebar,
})
</script>

<template>
	<TooltipProvider :delay-duration="0">
		<div
			:style="{
        '--sidebar-width': SIDEBAR_WIDTH,
        '--sidebar-width-icon': SIDEBAR_WIDTH_ICON,
      }"
			:class="cn('group/sidebar-wrapper flex min-h-svh w-full text-sidebar-foreground has-[[data-variant=inset]]:bg-sidebar', props.class)"
		>
			<slot />
		</div>
	</TooltipProvider>
</template>
