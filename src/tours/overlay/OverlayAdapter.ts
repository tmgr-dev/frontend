export interface OverlayShowOptions {
  anchorEl: Element | null;
  mode: 'modal' | 'passive' | 'centered';
  blocking?: boolean;
  mount: (host: HTMLElement) => () => void;
}

export interface OverlayAdapter {
  show(options: OverlayShowOptions): void;
  refresh(): void;
  hide(): void;
}
