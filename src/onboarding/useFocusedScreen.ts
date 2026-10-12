import { computed, onBeforeUnmount, onMounted, type ComputedRef, type Ref } from 'vue';
import { useRoute } from 'vue-router';
import { shouldShowConsentPanel } from '@/privacy/panelVisibility';
import { useConsent } from '@/privacy/useConsent';
import { inertAround } from '@/tours/a11y';
import { isDesktopApp } from '@/utils/desktop';

const KEEP_SELECTORS = ['[data-consent-panel]', '[role="region"][aria-label^="Notifications"]'];

export function useFocusedScreen(root: Ref<HTMLElement | null>): { consentOpen: ComputedRef<boolean> } {
  const route = useRoute();
  const { consent } = useConsent();
  const desktop = import.meta.env.MODE === 'desktop' || isDesktopApp();
  const consentOpen = computed(() =>
    shouldShowConsentPanel({
      analytics: consent.value.analytics,
      usage: consent.value.usage,
      path: route.path,
      desktop,
    }),
  );

  let restore: (() => void) | null = null;
  onMounted(() => {
    restore = inertAround([
      root.value,
      ...KEEP_SELECTORS.map((selector) => document.querySelector(selector)),
    ]);
  });
  onBeforeUnmount(() => restore?.());
  return { consentOpen };
}
