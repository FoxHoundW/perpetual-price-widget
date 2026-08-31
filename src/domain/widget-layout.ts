export const WIDGET_DEFAULT_WIDTH = 392;
export const WIDGET_MIN_WIDTH = 300;
export const WIDGET_MAX_WIDTH = 2000;
export const WIDGET_FRAME_HEIGHT = 22;
export const WIDGET_ROW_HEIGHT = 44;

export function widgetHeightForRows(visibleRows: number): number {
  return WIDGET_FRAME_HEIGHT + visibleRows * WIDGET_ROW_HEIGHT;
}
