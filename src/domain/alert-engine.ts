import type {
  AlertPeriod,
  AlertRecord,
  Direction,
  Market,
  PriceSample,
  WatchItem,
} from "./types";

const MINUTE = 60_000;

export interface EvaluateAlertsInput {
  now: number;
  market: Market;
  symbol: string;
  price: number;
  samples: PriceSample[];
  watch: WatchItem;
  periods: AlertPeriod[];
  rolling24hChange?: number;
}

interface TriggerState {
  triggerPrice: number;
  cooldownUntil: number;
}

export class AlertEngine {
  private readonly states = new Map<string, TriggerState>();

  evaluate(input: EvaluateAlertsInput): AlertRecord[] {
    if (!input.watch.alertEnabled || input.price <= 0) return [];
    const alerts: AlertRecord[] = [];

    for (const period of input.periods) {
      const baseline = this.findBaseline(input.samples, input.now - period * MINUTE);

      for (const direction of ["up", "down"] as const) {
        const key = this.stateKey(input.market, input.symbol, period, direction);
        const state = this.states.get(key);
        if (state && input.now < state.cooldownUntil) continue;

        const changePercent = state
          ? ((input.price - state.triggerPrice) / state.triggerPrice) * 100
          : period === 1440 && Number.isFinite(input.rolling24hChange)
            ? input.rolling24hChange!
            : baseline
              ? ((input.price - baseline.price) / baseline.price) * 100
              : Number.NaN;
        if (!Number.isFinite(changePercent)) continue;
        const threshold = direction === "up" ? input.watch.riseThreshold : input.watch.fallThreshold;
        const reached = direction === "up" ? changePercent >= threshold : changePercent <= -threshold;
        if (!reached) continue;

        this.states.set(key, {
          triggerPrice: input.price,
          cooldownUntil: input.now + 5 * MINUTE,
        });
        alerts.push(this.createAlert(input, period, direction, changePercent));
      }
    }

    return alerts;
  }

  resetSymbol(market: Market, symbol: string): void {
    const prefix = `${market}:${symbol}:`;
    for (const key of this.states.keys()) {
      if (key.startsWith(prefix)) this.states.delete(key);
    }
  }

  exportState(): Record<string, TriggerState> {
    return Object.fromEntries(this.states.entries());
  }

  restoreState(input: Record<string, TriggerState>): void {
    this.states.clear();
    for (const [key, state] of Object.entries(input)) {
      if (state.triggerPrice > 0 && Number.isFinite(state.cooldownUntil)) this.states.set(key, state);
    }
  }

  private findBaseline(samples: PriceSample[], targetTime: number): PriceSample | undefined {
    let candidate: PriceSample | undefined;
    for (const sample of samples) {
      if (sample.timestamp <= targetTime && (!candidate || sample.timestamp > candidate.timestamp)) {
        candidate = sample;
      }
    }
    return candidate;
  }

  private createAlert(
    input: EvaluateAlertsInput,
    period: AlertPeriod,
    direction: Direction,
    changePercent: number,
  ): AlertRecord {
    return {
      id: `${input.market}-${input.symbol}-${period}-${direction}-${input.now}`,
      market: input.market,
      symbol: input.symbol,
      period,
      direction,
      changePercent,
      triggerPrice: input.price,
      triggerTime: input.now,
      expiresAt: input.now + 6 * 60 * MINUTE,
      dismissed: false,
    };
  }

  private stateKey(market: Market, symbol: string, period: AlertPeriod, direction: Direction): string {
    return `${market}:${symbol}:${period}:${direction}`;
  }
}
