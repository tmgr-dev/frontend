import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, type ComputedRef, type Ref } from 'vue';
import { useRoute } from 'vue-router';
import { shouldShowConsentPanel } from '@/privacy/panelVisibility';
import { useConsent } from '@/privacy/useConsent';
import { inertAround } from '@/tours/a11y';
import { isDesktopApp } from '@/utils/desktop';

const KEEP_SELECTORS = ['[data-consent-panel]', '[role="region"][aria-label^="Notifications"]'];

export function useFocusedScreen(root: Ref<HTMLElement | null>): {
  consentOpen: ComputedRef<boolean>;
  consentPad: Ref<number>;
} {
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

  const consentPad = ref(0);
  let observer: ResizeObserver | null = null;
  const track = async () => {
    observer?.disconnect();
    observer = null;
    if (!consentOpen.value) return;
    await nextTick();
    const panel = document.querySelector<HTMLElement>('[data-consent-panel]');
    if (!panel || typeof ResizeObserver === 'undefined') return;
    observer = new ResizeObserver(() => {
      consentPad.value = panel.offsetHeight;
    });
    observer.observe(panel);
  };
  watch(consentOpen, track);

  let restore: (() => void) | null = null;
  onMounted(() => {
    void track();
    restore = inertAround([
      root.value,
      ...KEEP_SELECTORS.map((selector) => document.querySelector(selector)),
    ]);
  });
  onBeforeUnmount(() => {
    restore?.();
    observer?.disconnect();
  });
  return { consentOpen, consentPad };
}
