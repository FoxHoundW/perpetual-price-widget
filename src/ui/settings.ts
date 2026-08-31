import { getCurrentWindow } from "@tauri-apps/api/window";
import { ALERT_PERIODS, DEFAULT_SETTINGS, DEMO_CONTRACTS } from "../domain/defaults";
import { normalizeSettings } from "../domain/defaults";
import type { AlertPeriod, AppSettings, Contract, WatchItem } from "../domain/types";
import { WIDGET_DEFAULT_WIDTH, widgetHeightForRows } from "../domain/widget-layout";
import type { RuntimeLogRow } from "../runtime";
import { escapeHtml, marketLabel, periodLabel } from "./shared";

type SettingsPage = "basic" | "personalization" | "pairs" | "alerts" | "proxy" | "logs" | "about";

const navItems: Array<{ id: SettingsPage; label: string; hint: string }> = [
  { id: "basic", label: "基础设置", hint: "启动、窗口与刷新" },
  { id: "personalization", label: "个性化", hint: "颜色与透明度" },
  { id: "pairs", label: "交易对管理", hint: "自选与官方列表" },
  { id: "alerts", label: "异动提醒", hint: "周期、阈值与展示" },
  { id: "proxy", label: "网络代理", hint: "连接方式" },
  { id: "logs", label: "日志", hint: "错误与恢复记录" },
  { id: "about", label: "关于", hint: "版本与许可证" },
];

interface SettingsState {
  page: SettingsPage;
  settings: AppSettings;
  watchlist: WatchItem[];
  contracts: Contract[];
  search: string;
  marketFilter: "all" | "usdm" | "coinm";
  logs: RuntimeLogRow[] | null;
  catalogSyncedAt: number | null;
}

export interface SettingsBindings {
  initialSettings?: AppSettings;
  contracts?: Contract[];
  catalogSyncedAt?: number | null;
  onSave?: (settings: AppSettings, forceReconnect?: boolean) => Promise<AppSettings>;
  onSync?: () => Promise<{ contracts: Contract[]; syncedAt: number | null }>;
  onLoadLogs?: () => Promise<RuntimeLogRow[]>;
  onClearLogs?: () => Promise<void>;
  onSaveProxy?: (username: string, password: string) => Promise<void>;
  onTestProxy?: (proxy: AppSettings["proxy"], password: string) => Promise<string>;
  onClose?: () => Promise<void>;
}

type PersistSettings = (
  settings: AppSettings,
  onLatest: (saved: AppSettings) => void,
  forceReconnect?: boolean,
) => void;

export function renderSettingsWindow(): string {
  return `
    <section class="settings-window" aria-label="设置">
      <header class="settings-titlebar" data-tauri-drag-region>
        <div class="app-mark" aria-hidden="true"><span></span><span></span><span></span></div>
        <div><strong>永续价格悬浮窗</strong><span>设置</span></div>
        <button class="icon-button settings-close" type="button" aria-label="关闭设置">×</button>
      </header>
      <div class="settings-layout">
        <nav class="settings-nav" aria-label="设置分类">
          ${navItems
            .map(
              (item, index) => `
                <button class="nav-item ${index === 0 ? "is-active" : ""}" type="button" data-page="${item.id}">
                  <span class="nav-indicator" aria-hidden="true"></span>
                  <span><strong>${item.label}</strong><small>${item.hint}</small></span>
                </button>`,
            )
            .join("")}
          <div class="nav-connection"><span class="status-pip live"></span><div><strong>行情正常</strong><small>U 本位 · 币本位</small></div></div>
        </nav>
        <main class="settings-content scroll-surface" id="settings-content"></main>
      </div>
    </section>`;
}

export function bindSettingsWindow(root: HTMLElement, bindings: SettingsBindings = {}): void {
  const settings = normalizeSettings(bindings.initialSettings ?? DEFAULT_SETTINGS);
  const state: SettingsState = {
    page: "basic",
    settings,
    watchlist: settings.watchlist.map((item) => ({ ...item })),
    contracts: bindings.contracts ?? DEMO_CONTRACTS,
    search: "",
    marketFilter: "all",
    logs: bindings.onLoadLogs ? [] : null,
    catalogSyncedAt: bindings.catalogSyncedAt ?? null,
  };
  const content = root.querySelector<HTMLElement>("#settings-content");
  if (!content) return;
  let saveQueue: Promise<AppSettings> = Promise.resolve(settings);
  let latestSaveRevision = 0;

  const persistSettings: PersistSettings = (nextSettings, onLatest, forceReconnect = false) => {
    const revision = ++latestSaveRevision;
    const snapshot = normalizeSettings(structuredClone(nextSettings));
    saveQueue = saveQueue
      .catch(() => snapshot)
      .then(() => bindings.onSave?.(snapshot, forceReconnect) ?? snapshot);
    void saveQueue.then((saved) => {
      if (revision === latestSaveRevision) onLatest(saved);
    });
  };

  root.querySelector<HTMLButtonElement>(".settings-close")?.addEventListener("click", () => {
    if (bindings.onClose) {
      void bindings.onClose();
      return;
    }
    if ("__TAURI_INTERNALS__" in window) {
      void getCurrentWindow().hide();
    }
  });

  const render = (): void => {
    content.innerHTML = renderPage(state);
    bindPage(content, state, render, bindings, persistSettings);
    root.querySelectorAll<HTMLButtonElement>(".nav-item").forEach((button) => {
      const isActive = button.dataset.page === state.page;
      button.classList.toggle("is-active", isActive);
      button.setAttribute("aria-current", isActive ? "page" : "false");
    });
  };

  root.querySelectorAll<HTMLButtonElement>(".nav-item").forEach((button) => {
    button.addEventListener("click", async () => {
      state.page = button.dataset.page as SettingsPage;
      render();
      content.scrollTop = 0;
      if (state.page === "logs" && bindings.onLoadLogs) {
        try {
          state.logs = await bindings.onLoadLogs();
          render();
        } catch {
          state.logs = [];
          render();
        }
      }
    });
  });
  render();
}

function renderPage(state: SettingsState): string {
  switch (state.page) {
    case "basic":
      return renderBasic(state.settings);
    case "personalization":
      return renderPersonalization(state.settings);
    case "pairs":
      return renderPairs(state);
    case "alerts":
      return renderAlerts(state);
    case "proxy":
      return renderProxy(state.settings);
    case "logs":
      return renderLogs(state.logs);
    case "about":
      return renderAbout();
  }
}

function pageHead(title: string, description: string, action = ""): string {
  return `<div class="page-head"><div><h1>${title}</h1><p>${description}</p></div>${action}</div>`;
}

function renderBasic(settings: AppSettings): string {
  return `
    ${pageHead("基础设置", "调整悬浮窗的启动、显示和刷新行为。")}
    <section class="settings-section">
      <div class="section-heading"><h2>启动与窗口</h2><p>更改会立即保存并应用。</p></div>
      ${toggleField("开机自动启动", "登录 Windows 后自动运行并驻留系统托盘。", "autostart", settings.autostart)}
      ${toggleField("始终置顶", "让行情悬浮窗保持在其他窗口上方。", "always-top", settings.alwaysOnTop)}
      <div class="field-row">
        <div><label for="visible-rows">可视行数</label><p>窗口高度会跟随完整行情行吸附。</p></div>
        <div class="stepper"><button type="button" aria-label="减少行数">−</button><input id="visible-rows" type="number" value="${settings.visibleRows}" min="1" max="20"><button type="button" aria-label="增加行数">+</button></div>
      </div>
      <div class="field-row">
        <div><label for="refresh-ms">价格刷新周期</label><p>行情持续接收，界面按此间隔显示最新值。</p></div>
        <div class="input-suffix"><input id="refresh-ms" type="number" value="${settings.refreshIntervalMs}" min="100" max="2000"><span>ms</span></div>
      </div>
    </section>
    <section class="settings-section compact-section">
      <div class="section-heading"><h2>窗口状态</h2><p>当前尺寸 ${Math.round(settings.windowGeometry.width)} × ${widgetHeightForRows(settings.visibleRows)}，显示 ${settings.visibleRows} 行。</p></div>
      <div class="inline-actions"><button class="secondary-button reset-size" type="button">恢复默认尺寸</button><span class="saved-note"><span></span>所有更改已保存</span></div>
    </section>`;
}

function toggleField(title: string, description: string, name: string, checked: boolean): string {
  return `<div class="field-row"><div><label for="${name}">${title}</label><p>${description}</p></div><label class="switch"><input id="${name}" type="checkbox" ${checked ? "checked" : ""}><span></span></label></div>`;
}

function colorField(title: string, description: string, id: string, value: string): string {
  return `<div class="field-row appearance-field"><div><label for="${id}-text">${title}</label><p>${description}</p></div><div class="color-control"><input id="${id}" type="color" value="${value}" aria-label="${title}"><input id="${id}-text" class="color-text" type="text" value="${value}" maxlength="7" spellcheck="false" aria-label="${title}十六进制值"></div></div>`;
}

function opacityField(
  title: string,
  description: string,
  id: string,
  value: number,
  minimum: number,
): string {
  return `<div class="field-row appearance-field"><div><label for="${id}">${title}</label><p>${description}</p></div><div class="opacity-control"><input id="${id}" type="range" value="${value}" min="${minimum}" max="100" step="1"><output for="${id}">${value}%</output></div></div>`;
}

function sizeField(
  title: string,
  description: string,
  id: string,
  value: number,
): string {
  return `<div class="field-row appearance-field"><div><label for="${id}">${title}</label><p>${description}</p></div><div class="opacity-control"><input id="${id}" type="range" value="${value}" min="0" max="6" step="0.5"><output for="${id}">${value}px</output></div></div>`;
}

function renderPersonalization(settings: AppSettings): string {
  const appearance = settings.appearance;
  return `
    ${pageHead("个性化", "调整主体和异动警示的颜色与透明度。")}
    <section class="settings-section">
      <div class="section-heading"><h2>主体外观</h2><p>背景保留轻微顶部高光，边框和阴影不随透明度改变。</p></div>
      ${colorField("背景颜色", "设置悬浮窗主体底色。", "background-color", appearance.backgroundColor)}
      ${opacityField("背景透明度", "只影响主体背景，不影响边框。", "background-opacity", appearance.backgroundOpacity, 0)}
      ${opacityField("字体透明度", "同时影响主体行情和侧边警示文字。", "font-opacity", appearance.fontOpacity, 30)}
      ${sizeField("字体阴影大小", "0px 关闭阴影；增大可提升透明背景下的文字分离度。", "font-shadow-size", appearance.fontShadowSize)}
    </section>
    <section class="settings-section">
      <div class="section-heading"><h2>异动外观</h2><p>上涨与下跌颜色同时用于主体警示和侧边卡片。</p></div>
      ${colorField("上涨警示颜色", "用于上涨价格、色条和上涨警示卡。", "alert-up-color", appearance.alertUpColor)}
      ${colorField("下跌警示颜色", "用于下跌价格、色条和下跌警示卡。", "alert-down-color", appearance.alertDownColor)}
      ${opacityField("警示强调透明度", "控制色条、警示名称和卡片方向色。", "alert-accent-opacity", appearance.alertAccentOpacity, 20)}
      ${colorField("警示弹框背景颜色", "设置所有侧边警示卡的统一底色。", "alert-card-background-color", appearance.alertCardBackgroundColor)}
      ${opacityField("警示弹框背景透明度", "只影响警示卡背景，不改变文字透明度。", "alert-card-background-opacity", appearance.alertCardBackgroundOpacity, 20)}
    </section>
    <section class="settings-section compact-section">
      <div class="section-heading"><h2>恢复默认</h2><p>只重置本页外观，不影响其他设置。</p></div>
      <div class="inline-actions"><button class="secondary-button reset-appearance" type="button">恢复默认外观</button><span class="saved-note"><span></span>更改会立即保存</span></div>
    </section>`;
}

function renderPairs(state: SettingsState): string {
  const selected = state.watchlist;
  const availableRows = renderAvailablePairRows(state);
  const syncLabel = state.catalogSyncedAt
    ? `最近成功同步：${new Date(state.catalogSyncedAt).toLocaleString("zh-CN", { hour12: false })}`
    : "尚无成功同步记录";

  return `
    ${pageHead("交易对管理", "搜索官方永续合约，并决定悬浮窗显示哪些交易对。", '<button class="secondary-button sync-button" type="button"><span class="sync-glyph">↻</span>立即同步</button>')}
    <div class="sync-meta"><span class="status-pip live"></span>${syncLabel} · 当前 ${state.contracts.length} 个永续合约</div>
    <div class="pair-manager">
      <section class="pair-pane selected-pane">
        <header><div><h2>已展示</h2><span>${selected.length} 个交易对</span></div><small>拖动调整顺序</small></header>
        <div class="pair-list scroll-surface">
          ${selected
            .map(
              (item, index) => `<div class="pair-row selected" draggable="true" data-pair-key="${item.market}:${escapeHtml(item.symbol)}">
                <span class="drag-handle" aria-hidden="true">⠿</span>
                <span class="order-index">${index + 1}</span>
                <div class="pair-name"><strong>${escapeHtml(item.symbol)}</strong><small>${marketLabel(item.market)}</small></div>
                <button class="icon-button remove-pair" type="button" data-market="${item.market}" data-symbol="${escapeHtml(item.symbol)}" aria-label="移除 ${escapeHtml(item.symbol)}">×</button>
              </div>`,
            )
            .join("")}
        </div>
      </section>
      <section class="pair-pane catalog-pane">
        <header><div><h2>全部永续合约</h2><span>来自币安官方列表</span></div></header>
        <div class="catalog-tools">
          <label class="search-box"><span aria-hidden="true">⌕</span><input type="search" id="pair-search" value="${escapeHtml(state.search)}" placeholder="搜索交易对" autocomplete="off"></label>
          <div class="filter-tabs" role="group" aria-label="市场筛选">
            ${([['all', '全部'], ['usdm', 'U 本位'], ['coinm', '币本位']] as const).map(([id, label]) => `<button type="button" data-market="${id}" class="${state.marketFilter === id ? "is-active" : ""}">${label}</button>`).join("")}
          </div>
        </div>
        <div class="pair-list catalog-list scroll-surface">${availableRows}</div>
      </section>
    </div>`;
}

export function renderAvailablePairRows(state: Pick<SettingsState, "contracts" | "watchlist" | "marketFilter" | "search">): string {
  const available = state.contracts.filter(
    (contract) =>
      !state.watchlist.some((item) => item.market === contract.market && item.symbol === contract.symbol) &&
      contract.status === "TRADING" &&
      (state.marketFilter === "all" || contract.market === state.marketFilter) &&
      contract.symbol.toLowerCase().includes(state.search.toLowerCase()),
  );
  return available.length
    ? available.map((contract) => `<div class="pair-row"><div class="asset-monogram">${escapeHtml(contract.baseAsset.slice(0, 1))}</div><div class="pair-name"><strong>${escapeHtml(contract.symbol)}</strong><small>${marketLabel(contract.market)} · 永续</small></div><button class="add-pair" type="button" data-market="${contract.market}" data-symbol="${escapeHtml(contract.symbol)}">添加</button></div>`).join("")
    : '<div class="empty-state"><strong>没有匹配的交易对</strong><span>尝试其他关键词或切换市场。</span></div>';
}

function renderAlerts(state: SettingsState): string {
  const alertSettings = state.settings.alerts;
  return `
    ${pageHead("异动提醒", "选择监测周期，并为每个交易对设置独立阈值。")}
    <section class="settings-section">
      <div class="section-heading"><h2>全局规则</h2><p>冷静期固定 5 分钟，警示效果保留 6 小时。</p></div>
      <div class="field-row period-field"><div><span class="field-label">监测周期</span><p>可同时启用多个周期。</p></div><div class="check-chips">${ALERT_PERIODS.map((period) => `<label><input type="checkbox" data-period="${period}" ${alertSettings.periods.includes(period) ? "checked" : ""}><span>${periodLabel(period)}</span></label>`).join("")}</div></div>
      <div class="field-row stacked-field"><div><span class="field-label">主体隐藏时</span><p>选择异动触发后的展示方式。</p></div><div class="radio-cards">
        <label><input type="radio" name="hidden-alert" value="side-card" ${alertSettings.hiddenBehavior === "side-card" ? "checked" : ""}><span><strong>显示侧边警示</strong><small>在悬浮窗上次位置附近显示，主体仍保持隐藏。</small></span></label>
        <label><input type="radio" name="hidden-alert" value="record-only" ${alertSettings.hiddenBehavior === "record-only" ? "checked" : ""}><span><strong>仅记录警示</strong><small>重新显示主体后，再查看侧边卡片堆。</small></span></label>
      </div></div>
    </section>
    <section class="settings-section alert-symbols">
      <div class="section-heading"><h2>单交易对设置</h2><p>阈值应用于上方全部已启用周期，修改后自动保存。</p></div>
      <div class="alert-table-head"><span>交易对</span><span>提醒</span><span>上涨阈值</span><span>下跌阈值</span></div>
      ${state.watchlist.map((item) => `<div class="alert-setting-row" data-alert-key="${item.market}:${escapeHtml(item.symbol)}"><div class="pair-name"><strong>${escapeHtml(item.symbol)}</strong><small>${marketLabel(item.market)}</small></div><label class="switch small"><input class="symbol-alert-enabled" type="checkbox" ${item.alertEnabled ? "checked" : ""}><span></span></label><div class="input-suffix compact"><input class="rise-threshold" type="number" value="${item.riseThreshold}" min="0.1" step="0.1"><span>%</span></div><div class="input-suffix compact"><input class="fall-threshold" type="number" value="${item.fallThreshold}" min="0.1" step="0.1"><span>%</span></div></div>`).join("")}
    </section>`;
}

export function parseAlertThreshold(value: string): number | null {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0.1 ? parsed : null;
}

export function mergeWatchlistInPlace(target: WatchItem[], saved: WatchItem[]): void {
  const current = new Map(target.map((item) => [`${item.market}:${item.symbol}`, item]));
  const merged = saved.map((item) => {
    const existing = current.get(`${item.market}:${item.symbol}`);
    if (!existing) return { ...item };
    Object.assign(existing, item);
    return existing;
  });
  target.splice(0, target.length, ...merged);
}

function renderProxy(settings: AppSettings): string {
  return `
    ${pageHead("网络代理", "为币安行情与交易对同步选择连接方式。")}
    <section class="settings-section">
      <div class="section-heading"><h2>代理模式</h2><p>手动代理失败时不会静默改为直连。</p></div>
      <div class="segmented-control proxy-mode" role="group" aria-label="代理模式"><button class="${settings.proxy.mode === "system" ? "is-active" : ""}" type="button" data-proxy="system">跟随系统</button><button class="${settings.proxy.mode === "http" ? "is-active" : ""}" type="button" data-proxy="http">HTTP</button><button class="${settings.proxy.mode === "socks5" ? "is-active" : ""}" type="button" data-proxy="socks5">SOCKS5</button></div>
      <div class="proxy-system-state" ${settings.proxy.mode === "system" ? "" : "hidden"}><span class="status-pip live"></span><div><strong>跟随 Windows 系统代理</strong><p>系统未启用代理时使用直连；可通过下方按钮验证两个市场。</p></div></div>
      <div class="proxy-manual" ${settings.proxy.mode === "system" ? "hidden" : ""}><div class="form-grid"><label><span>代理地址</span><input id="proxy-host" type="text" value="${escapeHtml(settings.proxy.host)}" placeholder="127.0.0.1"></label><label><span>端口</span><input id="proxy-port" type="number" min="1" max="65535" value="${settings.proxy.port ?? ""}" placeholder="7890"></label><label><span>用户名（可选）</span><input id="proxy-username" type="text" value="${escapeHtml(settings.proxy.username)}" autocomplete="username"></label><label><span>密码（可选）</span><input id="proxy-password" type="password" autocomplete="current-password"></label></div></div>
      <p class="proxy-result" role="status" aria-live="polite"></p>
      <div class="section-footer"><button class="secondary-button test-proxy" type="button">测试连接</button><button class="primary-button" type="button">保存并重新连接</button></div>
    </section>`;
}

function renderLogs(logs: RuntimeLogRow[] | null): string {
  const rows = logs === null
    ? renderDemoLogs()
    : logs.length
      ? logs.map(renderLogRow).join("")
      : '<div class="empty-state logs-empty"><strong>暂无日志</strong><span>新的错误、警告和恢复信息会显示在这里。</span></div>';
  return `
    ${pageHead("日志", "查看网络、同步和本地存储问题，以及恢复结果。", '<button class="danger-quiet clear-logs" type="button">清空日志</button>')}
    <div class="log-toolbar"><div class="filter-tabs"><button class="is-active" type="button">全部</button><button type="button">错误</button><button type="button">警告</button><button type="button">恢复</button></div><div><button class="quiet-button" type="button">复制选中</button><button class="quiet-button" type="button">导出</button></div></div>
    <section class="log-list">${rows}</section>
    <div class="log-retention"><span>保留最近 7 天，最大 10MB</span><span>第 1 页 · 共 ${logs?.length ?? 3} 条</span></div>`;
}

function renderDemoLogs(): string {
  return `
    <article class="log-entry error"><span class="log-level"></span><div class="log-time"><strong>14:28:10</strong><small>14:28:35</small></div><div class="log-message"><div><strong>COIN-M 行情连接失败</strong><span class="count-badge">×6</span></div><p>连接超时；正在每 5 秒重新连接。U 本位行情不受影响。</p><small>COIN-M WebSocket · TIMEOUT</small></div></article>
    <article class="log-entry recovered"><span class="log-level"></span><div class="log-time"><strong>14:28:40</strong></div><div class="log-message"><div><strong>COIN-M 行情已恢复</strong></div><p>订阅已经恢复，异动监测将在补齐基线后继续。</p><small>COIN-M WebSocket · RECOVERED</small></div></article>
    <article class="log-entry warning"><span class="log-level"></span><div class="log-time"><strong>09:00:02</strong></div><div class="log-message"><div><strong>交易对列表同步失败</strong></div><p>正在使用昨天 09:00 的有效列表；可检查网络后立即同步。</p><small>合约目录 · HTTP_451</small></div></article>`;
}

function renderLogRow(log: RuntimeLogRow): string {
  const tone = log.recovered ? "recovered" : log.level;
  const title = log.recovered ? `${log.source} 已恢复` : `${log.source} 发生错误`;
  const first = new Date(log.firstAt).toLocaleTimeString("zh-CN", { hour12: false });
  const last = new Date(log.lastAt).toLocaleTimeString("zh-CN", { hour12: false });
  return `<article class="log-entry ${tone}" data-log-id="${log.id}"><span class="log-level"></span><div class="log-time"><strong>${first}</strong>${log.count > 1 ? `<small>${last}</small>` : ""}</div><div class="log-message"><div><strong>${escapeHtml(title)}</strong>${log.count > 1 ? `<span class="count-badge">×${log.count}</span>` : ""}</div><p>${escapeHtml(log.message)}</p><small>${escapeHtml(log.source)} · ${escapeHtml(log.code)}</small></div></article>`;
}

export function renderAbout(): string {
  return `
    ${pageHead("关于", "轻量、只读的币安永续价格悬浮窗。")}
    <section class="about-hero"><div class="large-app-mark"><span></span><span></span><span></span></div><div><h2>永续价格悬浮窗</h2><p>版本 1.0.0</p></div></section>
    <section class="settings-section about-list"><div><span>许可证</span><strong>MIT 开源许可证</strong></div><div><span>数据来源</span><strong>Binance 公共行情接口</strong></div><div><span>权限说明</span><strong>不需要 API Key，不提供交易功能</strong></div><div><span>支持系统</span><strong>Windows 10 / Windows 11</strong></div></section>
    <div class="about-actions"><button class="secondary-button" type="button">查看 MIT 许可证</button><button class="secondary-button" type="button">第三方许可证</button></div>`;
}

function bindPage(
  root: HTMLElement,
  state: SettingsState,
  render: () => void,
  bindings: SettingsBindings,
  persistSettings: PersistSettings,
): void {
  const commit = (forceReconnect = false): void => {
    state.settings.watchlist = state.watchlist.map((item) => ({ ...item }));
    state.settings = normalizeSettings(state.settings);
    persistSettings(state.settings, (saved) => {
      mergeWatchlistInPlace(state.watchlist, saved.watchlist);
      state.settings = {
        ...saved,
        watchlist: state.watchlist.map((item) => ({ ...item })),
      };
    }, forceReconnect);
  };

  const autostart = root.querySelector<HTMLInputElement>("#autostart");
  autostart?.addEventListener("change", () => {
    state.settings.autostart = autostart.checked;
    commit();
  });
  const alwaysTop = root.querySelector<HTMLInputElement>("#always-top");
  alwaysTop?.addEventListener("change", () => {
    state.settings.alwaysOnTop = alwaysTop.checked;
    commit();
  });
  const rowInput = root.querySelector<HTMLInputElement>("#visible-rows");
  root.querySelectorAll<HTMLButtonElement>(".stepper button").forEach((button, index) => {
    button.addEventListener("click", () => {
      if (!rowInput) return;
      const delta = index === 0 ? -1 : 1;
      rowInput.value = String(Math.min(20, Math.max(1, Number(rowInput.value) + delta)));
      state.settings.visibleRows = Number(rowInput.value);
      commit();
    });
  });
  rowInput?.addEventListener("change", () => {
    state.settings.visibleRows = Number(rowInput.value);
    commit();
    rowInput.value = String(state.settings.visibleRows);
  });
  const refresh = root.querySelector<HTMLInputElement>("#refresh-ms");
  refresh?.addEventListener("change", () => {
    state.settings.refreshIntervalMs = Number(refresh.value);
    commit();
    refresh.value = String(state.settings.refreshIntervalMs);
  });
  root.querySelector<HTMLButtonElement>(".reset-size")?.addEventListener("click", () => {
    state.settings.visibleRows = 3;
    state.settings.windowGeometry.width = WIDGET_DEFAULT_WIDTH;
    commit();
    render();
  });

  const bindColor = (id: string, key: keyof AppSettings["appearance"]): void => {
    const picker = root.querySelector<HTMLInputElement>(`#${id}`);
    const text = root.querySelector<HTMLInputElement>(`#${id}-text`);
    const apply = (value: string, save: boolean): void => {
      if (!/^#[0-9a-f]{6}$/i.test(value)) {
        text?.setAttribute("aria-invalid", "true");
        return;
      }
      text?.removeAttribute("aria-invalid");
      const normalized = value.toUpperCase();
      (state.settings.appearance[key] as string) = normalized;
      if (picker) picker.value = normalized;
      if (text) text.value = normalized;
      if (save) commit();
    };
    picker?.addEventListener("input", () => apply(picker.value, false));
    picker?.addEventListener("change", () => apply(picker.value, true));
    text?.addEventListener("change", () => apply(text.value.trim(), true));
  };
  bindColor("background-color", "backgroundColor");
  bindColor("alert-up-color", "alertUpColor");
  bindColor("alert-down-color", "alertDownColor");
  bindColor("alert-card-background-color", "alertCardBackgroundColor");

  const bindOpacity = (id: string, key: keyof AppSettings["appearance"]): void => {
    const input = root.querySelector<HTMLInputElement>(`#${id}`);
    const output = root.querySelector<HTMLOutputElement>(`output[for="${id}"]`);
    input?.addEventListener("input", () => {
      (state.settings.appearance[key] as number) = Number(input.value);
      if (output) output.value = `${input.value}%`;
    });
    input?.addEventListener("change", () => {
      (state.settings.appearance[key] as number) = Number(input.value);
      commit();
    });
  };
  bindOpacity("background-opacity", "backgroundOpacity");
  bindOpacity("font-opacity", "fontOpacity");
  bindOpacity("alert-accent-opacity", "alertAccentOpacity");
  bindOpacity("alert-card-background-opacity", "alertCardBackgroundOpacity");
  const fontShadowSize = root.querySelector<HTMLInputElement>("#font-shadow-size");
  const fontShadowOutput = root.querySelector<HTMLOutputElement>('output[for="font-shadow-size"]');
  fontShadowSize?.addEventListener("input", () => {
    state.settings.appearance.fontShadowSize = Number(fontShadowSize.value);
    if (fontShadowOutput) fontShadowOutput.value = `${fontShadowSize.value}px`;
  });
  fontShadowSize?.addEventListener("change", () => {
    state.settings.appearance.fontShadowSize = Number(fontShadowSize.value);
    commit();
  });
  root.querySelector<HTMLButtonElement>(".reset-appearance")?.addEventListener("click", () => {
    state.settings.appearance = { ...DEFAULT_SETTINGS.appearance };
    commit();
    render();
  });

  const search = root.querySelector<HTMLInputElement>("#pair-search");
  const bindCatalogAdds = (scope: ParentNode): void => {
    scope.querySelectorAll<HTMLButtonElement>(".add-pair").forEach((button) => {
      button.addEventListener("click", () => {
        const contract = state.contracts.find(
          (item) => item.market === button.dataset.market && item.symbol === button.dataset.symbol,
        );
        if (!contract) return;
        state.watchlist.push({
          market: contract.market,
          symbol: contract.symbol,
          alertEnabled: true,
          riseThreshold: 5,
          fallThreshold: 5,
        });
        commit();
        render();
      });
    });
  };
  search?.addEventListener("input", () => {
    state.search = search.value;
    const catalog = root.querySelector<HTMLElement>(".catalog-list");
    if (!catalog) return;
    catalog.innerHTML = renderAvailablePairRows(state);
    bindCatalogAdds(catalog);
  });
  root.querySelectorAll<HTMLButtonElement>("[data-market]").forEach((button) => {
    button.addEventListener("click", () => {
      state.marketFilter = button.dataset.market as SettingsState["marketFilter"];
      render();
    });
  });
  root.querySelectorAll<HTMLButtonElement>(".remove-pair").forEach((button) => {
    button.addEventListener("click", () => {
      state.watchlist = state.watchlist.filter(
        (item) => !(item.market === button.dataset.market && item.symbol === button.dataset.symbol),
      );
      commit();
      render();
    });
  });
  bindCatalogAdds(root);
  root.querySelectorAll<HTMLElement>("[data-pair-key]").forEach((row) => {
    row.addEventListener("dragstart", (event) => {
      event.dataTransfer?.setData("text/plain", row.dataset.pairKey ?? "");
      event.dataTransfer?.setDragImage(row, 24, 22);
      row.classList.add("is-dragging");
    });
    row.addEventListener("dragend", () => row.classList.remove("is-dragging"));
    row.addEventListener("dragover", (event) => {
      event.preventDefault();
      if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
    });
    row.addEventListener("drop", (event) => {
      event.preventDefault();
      const sourceKey = event.dataTransfer?.getData("text/plain");
      const targetKey = row.dataset.pairKey;
      if (!sourceKey || !targetKey || sourceKey === targetKey) return;
      const sourceIndex = state.watchlist.findIndex((item) => `${item.market}:${item.symbol}` === sourceKey);
      const targetIndex = state.watchlist.findIndex((item) => `${item.market}:${item.symbol}` === targetKey);
      if (sourceIndex < 0 || targetIndex < 0) return;
      const [moved] = state.watchlist.splice(sourceIndex, 1);
      if (!moved) return;
      state.watchlist.splice(targetIndex, 0, moved);
      commit();
      render();
    });
  });
  root.querySelector<HTMLButtonElement>(".sync-button")?.addEventListener("click", async (event) => {
    const button = event.currentTarget as HTMLButtonElement;
    if (!bindings.onSync) return;
    button.disabled = true;
    button.textContent = "正在同步…";
    try {
      const synced = await bindings.onSync();
      state.contracts = synced.contracts;
      state.catalogSyncedAt = synced.syncedAt;
      render();
    } catch {
      button.disabled = false;
      button.textContent = "同步失败，重试";
    }
  });
  root.querySelectorAll<HTMLInputElement>("[data-period]").forEach((input) => {
    input.addEventListener("change", () => {
      const periods = root.querySelectorAll<HTMLInputElement>("[data-period]:checked");
      state.settings.alerts.periods = Array.from(
        periods,
        (item) => Number(item.dataset.period) as AlertPeriod,
      );
      commit();
    });
  });
  root.querySelectorAll<HTMLInputElement>('input[name="hidden-alert"]').forEach((input) => {
    input.addEventListener("change", () => {
      if (!input.checked) return;
      state.settings.alerts.hiddenBehavior = input.value as "side-card" | "record-only";
      commit();
    });
  });
  root.querySelectorAll<HTMLElement>("[data-alert-key]").forEach((row) => {
    const item = state.watchlist.find(
      (candidate) => `${candidate.market}:${candidate.symbol}` === row.dataset.alertKey,
    );
    if (!item) return;
    const enabled = row.querySelector<HTMLInputElement>(".symbol-alert-enabled");
    const rise = row.querySelector<HTMLInputElement>(".rise-threshold");
    const fall = row.querySelector<HTMLInputElement>(".fall-threshold");
    enabled?.addEventListener("change", () => { item.alertEnabled = enabled.checked; commit(); });
    const bindThresholdAutosave = (
      input: HTMLInputElement | null,
      key: "riseThreshold" | "fallThreshold",
    ): void => {
      if (!input) return;
      let saveTimer: number | undefined;
      const saveValidValue = (immediate: boolean): void => {
        const value = parseAlertThreshold(input.value);
        input.setAttribute("aria-invalid", value === null ? "true" : "false");
        if (value === null) return;
        item[key] = value;
        if (saveTimer !== undefined) window.clearTimeout(saveTimer);
        if (immediate) {
          commit();
          return;
        }
        saveTimer = window.setTimeout(() => commit(), 250);
      };
      input.addEventListener("input", () => saveValidValue(false));
      input.addEventListener("change", () => saveValidValue(true));
    };
    bindThresholdAutosave(rise ?? null, "riseThreshold");
    bindThresholdAutosave(fall ?? null, "fallThreshold");
  });
  root.querySelectorAll<HTMLButtonElement>("[data-proxy]").forEach((button) => {
    button.addEventListener("click", () => {
      root.querySelectorAll("[data-proxy]").forEach((item) => item.classList.toggle("is-active", item === button));
      const isSystem = button.dataset.proxy === "system";
      const system = root.querySelector<HTMLElement>(".proxy-system-state");
      const manual = root.querySelector<HTMLElement>(".proxy-manual");
      if (system) system.hidden = !isSystem;
      if (manual) manual.hidden = isSystem;
      state.settings.proxy.mode = button.dataset.proxy as AppSettings["proxy"]["mode"];
    });
  });
  const readProxyForm = (): { username: string; password: string } => {
    const host = root.querySelector<HTMLInputElement>("#proxy-host");
    const port = root.querySelector<HTMLInputElement>("#proxy-port");
    const username = root.querySelector<HTMLInputElement>("#proxy-username");
    const password = root.querySelector<HTMLInputElement>("#proxy-password");
    if (host) state.settings.proxy.host = host.value.trim();
    if (port) state.settings.proxy.port = port.value
      ? Math.min(65535, Math.max(1, Math.round(Number(port.value))))
      : null;
    if (username) state.settings.proxy.username = username.value.trim();
    return { username: state.settings.proxy.username, password: password?.value ?? "" };
  };
  root.querySelector<HTMLButtonElement>(".primary-button")?.addEventListener("click", async () => {
    const { username, password } = readProxyForm();
    if (bindings.onSaveProxy) await bindings.onSaveProxy(username, password);
    commit(true);
  });
  root.querySelector<HTMLButtonElement>(".test-proxy")?.addEventListener("click", async (event) => {
    if (!bindings.onTestProxy) return;
    const button = event.currentTarget as HTMLButtonElement;
    const result = root.querySelector<HTMLElement>(".proxy-result");
    const { password } = readProxyForm();
    button.disabled = true;
    button.textContent = "正在测试…";
    try {
      const message = await bindings.onTestProxy(state.settings.proxy, password);
      if (result) result.textContent = message;
      button.textContent = "连接正常";
    } catch (error) {
      if (result) result.textContent = `连接失败：${String(error)}`;
      button.textContent = "重新测试";
    } finally {
      button.disabled = false;
    }
  });
  root.querySelector<HTMLButtonElement>(".clear-logs")?.addEventListener("click", () => {
    if (window.confirm("清空全部日志？此操作无法撤销。")) {
      void bindings.onClearLogs?.().then(() => {
        state.logs = [];
        render();
      });
      if (!bindings.onClearLogs) {
        const list = root.querySelector<HTMLElement>(".log-list");
        if (list) list.innerHTML = '<div class="empty-state logs-empty"><strong>暂无日志</strong><span>新的错误、警告和恢复信息会显示在这里。</span></div>';
      }
    }
  });
}
