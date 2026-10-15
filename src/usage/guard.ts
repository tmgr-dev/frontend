import { isDesktopApp } from '@/utils/desktop';

export const desktopOnly = (): true | string => (isDesktopApp() ? true : '/');
