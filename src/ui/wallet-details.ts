import type { WalletSnapshot } from "../runtime";
import { escapeHtml } from "./shared";

const walletNames: Record<string, string> = {
  Spot: "现货", Funding: "资金钱包", "Cross Margin (PM)": "全仓杠杆", "Cross Margin": "全仓杠杆",
  "Isolated Margin": "逐仓杠杆", "USDⓈ-M Futures (PM)": "U 本位合约", "USDⓈ-M Futures": "U 本位合约",
  "COIN-M Futures (PM)": "币本位合约", "COIN-M Futures": "币本位合约", Earn: "理财", Options: "期权",
  "Trading Bots": "交易机器人", "Copy Trading": "跟单交易",
};

export function renderWalletDetails(snapshot: WalletSnapshot | null): string {
  if (!snapshot || snapshot.balance === null || snapshot.error) {
    return '<div class="wallet-detail-empty">账号余额：---</div>';
  }
  return snapshot.details.map(item => `<div class="wallet-detail-row"><span>${escapeHtml(walletNames[item.name] ?? item.name)}</span><strong>${escapeHtml(item.balance)}</strong></div>`).join("")
    || '<div class="wallet-detail-empty">暂无钱包明细</div>';
}

export const walletDetailsShell = (): string => `<section class="wallet-details" role="dialog" aria-label="各钱包余额"><header><div><strong>各钱包余额</strong><small>USDT</small></div><button class="icon-button wallet-details-close" type="button" aria-label="关闭钱包明细">×</button></header><div class="wallet-detail-list"></div></section>`;
