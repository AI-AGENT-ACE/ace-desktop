import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

function appWindow() {
  if (!isTauri()) throw new Error('창 제어는 Tauri 데스크톱 앱에서 사용할 수 있습니다.');
  return getCurrentWindow();
}
export const windowAdapter = {
  available: isTauri,
  async minimize() { await appWindow().minimize(); },
  async toggleMaximize() { await appWindow().toggleMaximize(); },
  async close() { appWindow(); await invoke('hide_ace'); },
  async startDragging() { await appWindow().startDragging(); },
  async isMaximized() { return appWindow().isMaximized(); },
  async onResize(listener: () => void) { return appWindow().onResized(listener); },
};
