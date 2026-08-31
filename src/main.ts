import "./styles.css";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { DEFAULT_SETTINGS, DEMO_TICKERS } from "./domain/defaults";
import type { AlertRecord, AppSettings, Contract, Direction, TickerSnapshot } from "./domain/types";
import { symbolKey } from "./domain/types";
import { WIDGET_FRAME_HEIGHT, WIDGET_MIN_WIDTH, WIDGET_ROW_HEIGHT, widgetHeightForRows } from "./domain/widget-layout";
import { clearLogs, closeSettings, dismissAlert, exitApp, getAlerts, getAlertStates, getBootstrap, getLogs, getTickers, hideMain, isTauriRuntime, openSettings, saveProxyPassword, saveSettings, saveWindowGeometry, setMousePassthrough, syncContracts, testProxy } from "./runtime";
import { applyAppearanceVariables } from "./ui/appearance";
import { alertPanelSignature, alertStackKey, bindAlertPanel, groupAlerts, renderAlertPanel } from "./ui/alerts";
import type { AlertStack } from "./ui/alerts";
import { showWidgetContextMenu } from "./ui/context-menu";
import { createDemoAlertStacks } from "./ui/demo-data";
import { bindSettingsWindow, renderSettingsWindow } from "./ui/settings";
import { renderWidget } from "./ui/widget";

type View = "showcase" | "widget" | "settings" | "alerts";

const appNode = document.querySelector<HTMLElement>("#app");
if (!appNode) throw new Error("找不到应用挂载节点");
const app: HTMLElement = appNode;

const requested = new URLSearchParams(window.location.search).get("view");
const view: View = ["widget", "settings", "alerts"].includes(requested ?? "")
  ? (requested as View)
  : "showcase";

function orderedTickers(
  settings: AppSettings,
  tickers: TickerSnapshot[],
  contracts: Contract[],
): TickerSnapshot[] {
  return settings.watchlist.map((item) => {
    const ticker = tickers.find((candidate) => candidate.market === item.market && candidate.symbol === item.symbol);
    const contract = contracts.find((candidate) => candidate.market === item.market && candidate.symbol === item.symbol);
    return ticker ?? {
      market: item.market,
      symbol: item.symbol,
      price: 0,
      priceText: "—",
      change24h: 0,
      eventTime: 0,
      status: contracts.length > 0 && (!contract || contract.status === "DELISTED") ? "delisted" : "connecting",
    };
  });
}

function activeAlertSymbols(alerts: AlertRecord[]): Map<string, Direction> {
  const result = new Map<string, Direction>();
  for (const alert of [...alerts].sort((a, b) => a.triggerTime - b.triggerTime)) {
    if (alert.expiresAt > Date.now()) {
      result.set(symbolKey(alert.market, alert.symbol), alert.direction);
    }
  }
  return result;
}

async function mountWidget(): Promise<void> {
  document.body.className = "native-view widget-view";
  if (!isTauriRuntime) {
    applyAppearanceVariables(DEFAULT_SETTINGS.appearance);
    app.innerHTML = renderWidget({
      tickers: DEMO_TICKERS,
      alertSymbols: new Map([[symbolKey("usdm", "SOLUSDT"), "up"]]),
    });
    return;
  }
  try {
    const bootstrap = await getBootstrap();
    let settings = bootstrap.settings;
    applyAppearanceVariables(settings.appearance);
    let latestTickers = bootstrap.tickers;
    let latestAlerts = await getAlertStates();
    let mousePassthrough = false;
    let refreshTimer: number | undefined;
    const renderLatest = (tickers: TickerSnapshot[]): void => {
      const previousScroll = app.querySelector<HTMLElement>(".ticker-scroll")?.scrollTop ?? 0;
      latestTickers = tickers;
      const ordered = orderedTickers(settings, tickers, bootstrap.contracts);
      app.innerHTML = renderWidget({
        tickers: ordered,
        stale: ordered.some((ticker) => ticker.status !== "delisted")
          && ordered.every((ticker) => ["stale", "reconnecting", "delisted"].includes(ticker.status)),
        lockWindow: settings.lockWindow,
        alertSymbols: activeAlertSymbols(latestAlerts),
      });
      const nextScroll = app.querySelector<HTMLElement>(".ticker-scroll");
      if (nextScroll) {
        nextScroll.scrollTop = Math.min(
          previousScroll,
          Math.max(0, nextScroll.scrollHeight - nextScroll.clientHeight),
        );
      }
      app.querySelector(".ticker-widget")?.addEventListener("contextmenu", (event) => {
        event.preventDefault();
        void showWidgetContextMenu(settings, mousePassthrough, {
          update: async (patch) => {
            settings = await saveSettings({ ...settings, ...patch });
            renderLatest(latestTickers);
          },
          setMousePassthrough: async (enabled) => {
            await setMousePassthrough(enabled);
            mousePassthrough = enabled;
          },
          openSettings,
          hide: hideMain,
          exit: exitApp,
        });
      });
    };
    renderLatest(bootstrap.tickers);
    const refresh = async (): Promise<void> => {
      try {
        [latestTickers, latestAlerts] = await Promise.all([getTickers(), getAlertStates()]);
        renderLatest(latestTickers);
      } catch {
        app.querySelector(".connection-dot")?.classList.replace("live", "warning");
      } finally {
        refreshTimer = window.setTimeout(() => void refresh(), settings.refreshIntervalMs);
      }
    };
    refreshTimer = window.setTimeout(() => void refresh(), settings.refreshIntervalMs);
    await listen<AppSettings>("settings-updated", (event) => {
      settings = event.payload;
      applyAppearanceVariables(settings.appearance);
      if (refreshTimer !== undefined) window.clearTimeout(refreshTimer);
      renderLatest(latestTickers);
      refreshTimer = window.setTimeout(() => void refresh(), settings.refreshIntervalMs);
    });
    await listen<boolean>("mouse-passthrough-changed", (event) => {
      mousePassthrough = event.payload;
    });
    const currentWindow = getCurrentWindow();
    let resizeTimer: number | undefined;
    let snapping = false;
    await currentWindow.onResized(({ payload }) => {
      if (snapping || settings.lockSize || settings.lockWindow) return;
      if (resizeTimer !== undefined) window.clearTimeout(resizeTimer);
      resizeTimer = window.setTimeout(async () => {
        const logical = payload.toLogical(await currentWindow.scaleFactor());
        const visibleRows = Math.min(
          20,
          Math.max(1, Math.round((logical.height - WIDGET_FRAME_HEIGHT) / WIDGET_ROW_HEIGHT)),
        );
        const snappedHeight = widgetHeightForRows(visibleRows);
        settings = {
          ...settings,
          visibleRows,
          windowGeometry: {
            ...settings.windowGeometry,
            width: Math.max(WIDGET_MIN_WIDTH, logical.width),
          },
        };
        await saveWindowGeometry(
          visibleRows,
          settings.windowGeometry.width,
          settings.windowGeometry.x,
          settings.windowGeometry.y,
        );
        if (Math.abs(logical.height - snappedHeight) >= 1) {
          snapping = true;
          try {
            await currentWindow.setSize(new LogicalSize(settings.windowGeometry.width, snappedHeight));
          } finally {
            snapping = false;
          }
        }
      }, 180);
    });
    let moveTimer: number | undefined;
    await currentWindow.onMoved(({ payload }) => {
      if (settings.lockWindow) return;
      if (moveTimer !== undefined) window.clearTimeout(moveTimer);
      moveTimer = window.setTimeout(() => {
        settings = {
          ...settings,
          windowGeometry: { ...settings.windowGeometry, x: payload.x, y: payload.y },
        };
        void saveWindowGeometry(
          settings.visibleRows,
          settings.windowGeometry.width,
          payload.x,
          payload.y,
        );
      }, 250);
    });
  } catch {
    app.innerHTML = `
      <section class="ticker-widget widget-error-state">
        <div class="widget-drag-region" data-tauri-drag-region></div>
        <div class="compact-recovery"><strong>无法载入行情</strong><span>请检查网络，或在设置中配置代理。</span><button type="button" onclick="location.reload()">重试</button></div>
      </section>`;
  }
}

async function mountSettings(): Promise<void> {
  document.body.className = "native-view settings-view";
  app.innerHTML = renderSettingsWindow();
  if (!isTauriRuntime) {
    bindSettingsWindow(app);
    return;
  }
  try {
    const bootstrap = await getBootstrap();
    bindSettingsWindow(app, {
      initialSettings: bootstrap.settings,
      contracts: bootstrap.contracts,
      catalogSyncedAt: bootstrap.catalogSyncedAt,
      onSave: saveSettings,
      onSync: async () => {
        await syncContracts();
        const synced = await getBootstrap();
        return { contracts: synced.contracts, syncedAt: synced.catalogSyncedAt };
      },
      onLoadLogs: () => getLogs(0, 50),
      onClearLogs: clearLogs,
      onSaveProxy: saveProxyPassword,
      onTestProxy: testProxy,
      onClose: closeSettings,
    });
  } catch {
    app.innerHTML = `
      <section class="settings-window" aria-label="设置载入失败">
        <div class="settings-fatal"><strong>无法载入设置</strong><span>本地配置或数据库暂时不可用，请重新打开设置；详细原因会写入日志。</span><button class="secondary-button" type="button" onclick="location.reload()">重试</button></div>
      </section>`;
  }
}

async function mountAlerts(): Promise<void> {
  document.body.className = "native-view alerts-view";
  if (isTauriRuntime) {
    let stacks: AlertStack[] = [];
    let renderedSignature = "";
    const seenAlertIds = new Set<string>();
    const bootstrap = await getBootstrap();
    applyAppearanceVariables(bootstrap.settings.appearance);
    app.innerHTML = renderAlertPanel(stacks);
    const applyAlerts = (records: AlertRecord[]): void => {
      const nextStacks = groupAlerts(records, stacks);
      const nextSignature = alertPanelSignature(nextStacks);
      if (nextSignature === renderedSignature) return;
      const enteringKeys = new Set(
        nextStacks
          .filter((stack) => stack.cards.some((card) => !seenAlertIds.has(card.id)))
          .map(alertStackKey),
      );
      stacks = nextStacks;
      for (const stack of stacks) {
        for (const card of stack.cards) seenAlertIds.add(card.id);
      }
      app.innerHTML = renderAlertPanel(stacks, enteringKeys);
      renderedSignature = nextSignature;
      bindAlertPanel(app, stacks, dismissAlert, () => {
        renderedSignature = alertPanelSignature(stacks);
      });
    };
    const refresh = async (): Promise<void> => {
      try {
        applyAlerts(await getAlerts());
      } catch {
        // The backend reconnect loop owns diagnostics; keep the last visible stack.
      }
    };
    await refresh();
    await listen<AppSettings>("settings-updated", (event) => {
      applyAppearanceVariables(event.payload.appearance);
    });
    await listen<AlertRecord[]>("alerts-updated", (event) => {
      applyAlerts(event.payload);
    });
    window.setInterval(() => void refresh(), 1000);
    return;
  }
  const stacks = createDemoAlertStacks();
  applyAppearanceVariables(DEFAULT_SETTINGS.appearance);
  app.innerHTML = renderAlertPanel(stacks, new Set(stacks.map(alertStackKey)));
  bindAlertPanel(app, stacks);
}

function mountShowcase(): void {
  applyAppearanceVariables(DEFAULT_SETTINGS.appearance);
  document.body.className = "showcase-view";
  const stacks = createDemoAlertStacks();
  const demoAlertSymbols = activeAlertSymbols(stacks.flatMap((stack) => stack.cards));
  app.innerHTML = `
    <main class="showcase-shell">
      <header class="showcase-toolbar">
        <div class="showcase-brand"><span class="brand-signal"><i></i><i></i><i></i></span><div><strong>永续价格悬浮窗</strong><small>交互效果预览</small></div></div>
        <div class="showcase-tabs" role="tablist" aria-label="预览界面">
          <button type="button" class="is-active" data-demo-view="desktop">桌面组合</button>
          <button type="button" data-demo-view="settings">设置面板</button>
          <button type="button" data-demo-view="stale">断线状态</button>
        </div>
        <div class="demo-hint"><span></span>点击警示卡片可切换堆内内容</div>
      </header>
      <section class="desktop-preview" data-demo-panel="desktop">
        <div class="desktop-grain" aria-hidden="true"></div>
        <div class="desktop-caption"><strong>专注模式</strong><span>行情留在余光里，其他设置保持安静。</span></div>
        <div class="preview-composition">
          ${renderWidget({ tickers: DEMO_TICKERS, className: "demo-widget", alertSymbols: demoAlertSymbols })}
          <div class="demo-alert-host">${renderAlertPanel(stacks, new Set(stacks.map(alertStackKey)))}</div>
        </div>
        <div class="taskbar-preview"><div class="taskbar-apps"><span></span><span></span><span></span></div><div class="tray-preview"><i class="tray-app-icon"><b></b><b></b><b></b></i><span>14:32</span></div></div>
      </section>
      <section class="settings-preview" data-demo-panel="settings" hidden>${renderSettingsWindow()}</section>
      <section class="desktop-preview stale-preview" data-demo-panel="stale" hidden>
        <div class="desktop-grain" aria-hidden="true"></div>
        <div class="recovery-note"><span class="status-pip warning"></span><div><strong>COIN-M 行情暂时中断</strong><p>保留最后可信价格，每 5 秒重新连接；U 本位继续更新。</p></div></div>
        <div class="preview-composition solo">${renderWidget({ tickers: DEMO_TICKERS.map((ticker, index) => index === 1 ? { ...ticker, status: "stale" } : ticker), stale: true, className: "demo-widget" })}</div>
      </section>
    </main>`;

  const alertHost = app.querySelector<HTMLElement>(".demo-alert-host");
  if (alertHost) bindAlertPanel(alertHost, stacks);
  const settingsRoot = app.querySelector<HTMLElement>(".settings-preview");
  if (settingsRoot) bindSettingsWindow(settingsRoot);
  app.querySelectorAll<HTMLButtonElement>("[data-demo-view]").forEach((button) => {
    button.addEventListener("click", () => {
      const target = button.dataset.demoView;
      app.querySelectorAll<HTMLElement>("[data-demo-panel]").forEach((panel) => {
        panel.hidden = panel.dataset.demoPanel !== target;
      });
      app.querySelectorAll("[data-demo-view]").forEach((item) => item.classList.toggle("is-active", item === button));
    });
  });
}

switch (view) {
  case "widget":
    void mountWidget();
    break;
  case "settings":
    void mountSettings();
    break;
  case "alerts":
    void mountAlerts();
    break;
  case "showcase":
    mountShowcase();
    break;
}
