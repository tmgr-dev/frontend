import { ref, onMounted, onUnmounted } from 'vue';

export function useIsMobile(breakpoint: number = 768) {
	const isMobile = ref(false);

	const checkIsMobile = () => {
		isMobile.value = window.innerWidth <= breakpoint;
	};

	onMounted(() => {
		// Check initially
		checkIsMobile();

		// Add resize listener
		window.addEventListener('resize', checkIsMobile);
	});

	onUnmounted(() => {
		// Clean up
		window.removeEventListener('resize', checkIsMobile);
	});

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
