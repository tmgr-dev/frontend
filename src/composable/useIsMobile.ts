import { ref, onMounted, onUnmounted, getCurrentInstance } from 'vue';

export function useIsMobile(breakpoint: number = 768) {
	const isMobile = ref(false);

	const checkIsMobile = () => {
		isMobile.value = window.innerWidth <= breakpoint;
	};

	if (getCurrentInstance()) {
		onMounted(() => {
			checkIsMobile();
			window.addEventListener('resize', checkIsMobile);
		});

		onUnmounted(() => {
			window.removeEventListener('resize', checkIsMobile);
		});
	} else {
		checkIsMobile();
		if (typeof window !== 'undefined') {
			window.addEventListener('resize', checkIsMobile);
		}
	}

	return {
		isMobile,
	};
}

// Usage example:
/*
import { useIsMobile } from '@/composables/useIsMobile';

export default {
  setup() {
    // Default breakpoint (768px)
    const { isMobile } = useIsMobile();

    // Or custom breakpoint
    const { isMobile: isTablet } = useIsMobile(1024);

    return {
      isMobile,
      isTablet,
    };
  }
};
*/
