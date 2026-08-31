import type { LogEvent } from "./types";

const SECRET_PATTERNS = [
  /([?&](?:token|signature|key|password)=)[^&\s]+/gi,
  /(authorization\s*:\s*)[^\s,]+/gi,
  /(https?:\/\/)([^:@\s/]+):([^@\s/]+)@/gi,
];

export function sanitizeLogMessage(message: string): string {
  return SECRET_PATTERNS.reduce((value, pattern, index) => {
    if (index === 2) return value.replace(pattern, "$1***:***@");
    return value.replace(pattern, "$1***");
  }, message);
}

export class LogAggregator {
  private readonly events: LogEvent[] = [];
  private activeFingerprint: string | null = null;

  append(input: Omit<LogEvent, "id" | "firstAt" | "lastAt" | "count" | "recovered"> & { at: number }): LogEvent {
    const message = sanitizeLogMessage(input.message);
    const fingerprint = `${input.source}\u0000${input.code}\u0000${message}`;
    const latest = this.events.at(-1);

    if (latest && this.activeFingerprint === fingerprint && !latest.recovered) {
      latest.lastAt = input.at;
      latest.count += 1;
      return latest;
    }

    const event: LogEvent = {
      id: `${input.at}-${this.events.length}`,
      level: input.level,
      source: input.source,
      code: input.code,
      message,
      firstAt: input.at,
      lastAt: input.at,
      count: 1,
      recovered: false,
    };
    this.events.push(event);
    this.activeFingerprint = fingerprint;
    return event;
  }

  recover(source: string, message: string, at: number): LogEvent {
    const event: LogEvent = {
      id: `${at}-${this.events.length}`,
      level: "info",
      source,
      code: "RECOVERED",
      message: sanitizeLogMessage(message),
      firstAt: at,
      lastAt: at,
      count: 1,
      recovered: true,
    };
    this.events.push(event);
    this.activeFingerprint = null;
    return event;
  }

  list(): readonly LogEvent[] {
    return this.events;
  }
}
