import { describe, expect, it } from "vitest";
import { renderWalletDetails } from "./wallet-details";

describe("wallet details", () => {
  it("shows every wallet including zero balances with escaped names", () => {
    const html = renderWalletDetails({ balance: "12.00", wallets: [], error: null, updatedAt: 1, details: [
      {name:"Copy Trading", balance:"12.00"}, {name:"Spot", balance:"0.00"}, {name:"<custom>", balance:"0.00"},
    ] });
    expect(html).toContain("跟单交易");
    expect(html).toContain("现货");
    expect(html).toContain("0.00");
    expect(html).toContain("&lt;custom&gt;");
  });
  it("never shows old details when the current read failed", () => {
    expect(renderWalletDetails({balance:null, error:"网络失败", wallets:[], updatedAt:1, details:[{name:"Spot",balance:"99.00"}]})).not.toContain("99.00");
    expect(renderWalletDetails(null)).toContain("账号余额：---");
  });
});
