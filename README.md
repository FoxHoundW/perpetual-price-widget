# 永续价格悬浮窗

一个面向 Windows 10/11 的轻量币安永续合约价格悬浮窗。主体仅显示交易对、最新成交价和 24 小时涨跌幅，并提供可配置的异动提醒。

> 本项目与 Binance 无隶属、授权或背书关系。行情不需要 API Key；可选的账号余额功能需要账户读取凭据。软件不提供交易功能，也不构成投资建议。

## 主要功能

- 可选账号总余额（USDT 估值）及各钱包明细，默认每秒后台更新，点击余额展开明细。
- 总余额与钱包明细同步刷新，失败显示 ---；不提供仓位浮盈或网页登录。
- 同时支持 USDⓈ-M 与 COIN-M 交易中的永续合约。
- 每日自动刷新官方交易对目录，也可在设置中手动刷新。
- 默认 BTCUSDT、ETHUSDT、SOLUSDT 和 3 个可视行，支持 1–20 行。
- 最新成交价与 24h 涨跌幅，100–2000ms 可调显示刷新，默认 500ms。
- 5min、10min、1h、6h、12h、24h 异动周期；每个交易对可单独设置开关和阈值。
- 5 分钟同向冷静期、阶梯触发基准和 6 小时警示保留。
- 无边框悬浮窗、系统托盘、始终置顶、大小/位置锁定、鼠标穿透和开机自启。
- 背景、字体和警示组件的颜色/透明度，以及字体阴影调节。
- WebSocket Ping/Pong 心跳、5 秒网络重连和重复错误聚合日志。
- 系统代理、HTTP 代理与 SOCKS5 代理；凭据保存到 Windows Credential Manager。

## 下载与安装

请在 [GitHub Releases](https://github.com/FoxHoundW/perpetual-price-widget/releases) 下载最新版本：

- `Perpetual.Price.Widget_1.1.0_x64-setup.exe`：安装版，适合日常使用。
- `Perpetual.Price.Widget_1.1.0_x64-portable.exe`：便携版，可直接运行。
- `SHA256SUMS.txt`：用于校验下载文件完整性。

系统要求：Windows 10/11 x64 和 Microsoft Edge WebView2 Runtime。Windows 10 与 Windows 11 通常已包含 WebView2，安装程序也会在缺失时尝试下载。

当前发布程序未使用付费代码签名，Windows 可能显示“未知发布者”或 SmartScreen 提示。请仅从本仓库 Releases 页面下载，并根据 `SHA256SUMS.txt` 校验文件。

## 基本使用

- 右键悬浮窗可打开设置、锁定窗口、切换鼠标穿透或退出。
- 托盘菜单可显示/隐藏悬浮窗或退出。
- 鼠标穿透开启后，通过系统托盘菜单关闭穿透或进入设置。
- 设置中可管理交易对、异动阈值、外观、代理、开机自启和日志。
- 基础设置中可开启账号余额，并在本机填写具有账户读取权限的 API Key 与 Secret Key。不要将密钥发到 Issues、聊天或源码中。首次使用请核对钱包覆盖范围和官网总览。

## 数据与隐私

- 市场数据来自 Binance 公开合约接口。
- 设置、有界状态数据和日志保存在当前 Windows 用户的应用数据目录。
- 代理密码不保存在设置文件中，而是交由 Windows Credential Manager 保管。
- 行情不要求币安账户。可选的账号余额功能使用本机 Windows 凭据管理器保存 HMAC API Key 与 Secret Key，仅查询余额，不提交订单。

## 本地开发

需要 Node.js 20+、pnpm、Rust stable-msvc、Microsoft C++ Build Tools、Windows SDK 和 WebView2。

```powershell
pnpm install
pnpm dev
```

HTML 预览默认位于 <http://127.0.0.1:1420/>。生产 Tauri 窗口使用 `?view=widget`、`?view=settings` 和 `?view=alerts`。

构建 Windows 应用：

```powershell
pnpm tauri build
```

NSIS 安装包位于 `src-tauri/target/release/bundle/nsis/`，Release 主程序位于 `src-tauri/target/release/perpetual-price-widget.exe`。

## 验证

```powershell
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
```

完整产品约束见 `docs/superpowers/specs/2026-08-28-binance-perpetual-price-widget-design.md`，实施与验收记录见 `docs/implementation/`。

## 贡献与安全

功能建议和可复现的问题可以通过 GitHub Issues 提交。安全漏洞请按 [SECURITY.md](SECURITY.md) 私密报告。

## 许可证

本项目由 [MIT License](LICENSE) 授权，版权归 `FoxHoundW` 所有。第三方组件说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
