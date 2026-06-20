//! The meter engine HTTP client.

import { MeterError } from "./errors";
import type {
  Account,
  Balance,
  CreditSource,
  Invoice,
  LedgerEntry,
  LimitClass,
  ReserveOutcome,
  UsageEvent,
  UsageResult,
  UsageTokens,
  Uuid,
} from "./types";

type FetchLike = typeof fetch;

export interface MeterClientOptions {
  readonly baseUrl: string;
  /** Override the fetch implementation (defaults to the global `fetch`). */
  readonly fetch?: FetchLike;
}

export interface OpenAccountInput {
  readonly orgId: Uuid;
  readonly scope: string;
  readonly noOverdraft?: boolean;
  readonly parentId?: Uuid;
}

export interface GrantInput {
  readonly amount: string;
  readonly source: CreditSource;
  readonly idempotencyKey?: string;
}

export interface ReserveInput {
  readonly account: Uuid;
  readonly reservationId: Uuid;
  readonly amount: string;
  readonly limit: LimitClass;
  /** Optional hold expiry (RFC3339). An open hold past it is released by the engine's sweep; extend it
   * with `extendReservation` to keep a long-running reservation alive. */
  readonly expiresAt?: string;
}

export interface OpenLeaseInput {
  readonly parent: Uuid;
  readonly amount: string;
}

export interface RecordEventInput {
  readonly orgId: Uuid;
  readonly idempotencyKey: string;
  readonly meter: string;
  readonly account: Uuid;
  readonly runId?: Uuid;
  readonly properties?: Record<string, unknown>;
  readonly eventTime?: string;
}

export interface MeterUsageInput {
  readonly orgId: Uuid;
  readonly account: Uuid;
  readonly model: string;
  readonly idempotencyKey: string;
  readonly runId?: Uuid;
  readonly usage: UsageTokens;
}

interface EngineErrorBody {
  readonly error: string;
  readonly message: string;
}

/** A thin, drop-in client for the meter engine's HTTP API. */
export class MeterClient {
  readonly #baseUrl: string;
  readonly #fetch: FetchLike;

  constructor(options: MeterClientOptions) {
    this.#baseUrl = options.baseUrl.replace(/\/+$/, "");
    this.#fetch = options.fetch ?? fetch;
  }

  openAccount(input: OpenAccountInput): Promise<Account> {
    return this.#post<Account>("/v1/accounts", {
      org_id: input.orgId,
      scope: input.scope,
      no_overdraft: input.noOverdraft ?? false,
      parent_id: input.parentId ?? null,
    });
  }

  balance(account: Uuid): Promise<Balance> {
    return this.#get<Balance>(`/v1/accounts/${account}/balance`);
  }

  grant(account: Uuid, input: GrantInput): Promise<LedgerEntry> {
    return this.#post<LedgerEntry>(`/v1/accounts/${account}/grants`, {
      amount: input.amount,
      source: input.source,
      idempotency_key: input.idempotencyKey ?? null,
    });
  }

  entries(account: Uuid): Promise<readonly LedgerEntry[]> {
    return this.#get<readonly LedgerEntry[]>(`/v1/accounts/${account}/entries`);
  }

  reserve(input: ReserveInput): Promise<ReserveOutcome> {
    return this.#post<ReserveOutcome>("/v1/reservations", {
      account: input.account,
      reservation_id: input.reservationId,
      amount: input.amount,
      limit: input.limit,
      expires_at: input.expiresAt ?? null,
    });
  }

  settle(reservationId: Uuid, actual: string): Promise<LedgerEntry> {
    return this.#post<LedgerEntry>(`/v1/reservations/${reservationId}/settle`, { actual });
  }

  /** Push out a hold's expiry (RFC3339) — a heartbeat so a long-running reservation isn't swept. */
  async extendReservation(reservationId: Uuid, expiresAt: string): Promise<void> {
    await this.#send<unknown>("POST", `/v1/reservations/${reservationId}/extend`, {
      expires_at: expiresAt,
    });
  }

  async voidReservation(reservationId: Uuid): Promise<void> {
    await this.#send<unknown>("POST", `/v1/reservations/${reservationId}/void`, undefined);
  }

  /** Open a per-session lease: a child account funded by a conserving transfer from the parent. */
  openLease(input: OpenLeaseInput): Promise<Account> {
    return this.#post<Account>("/v1/leases", { parent: input.parent, amount: input.amount });
  }

  /** Close a lease, returning its unused balance to the parent; resolves to the credits returned. */
  async closeLease(leaseId: Uuid): Promise<string> {
    const body = await this.#post<{ returned: string }>(`/v1/leases/${leaseId}/close`, undefined);
    return body.returned;
  }

  recordEvent(input: RecordEventInput): Promise<UsageEvent> {
    return this.#post<UsageEvent>("/v1/events", {
      org_id: input.orgId,
      idempotency_key: input.idempotencyKey,
      meter: input.meter,
      account: input.account,
      run_id: input.runId ?? null,
      properties: input.properties ?? {},
      event_time: input.eventTime ?? null,
    });
  }

  amendEvent(eventId: Uuid, properties: Record<string, unknown>): Promise<UsageEvent> {
    return this.#post<UsageEvent>(`/v1/events/${eventId}/amend`, { properties });
  }

  listEvents(account: Uuid): Promise<readonly UsageEvent[]> {
    return this.#get<readonly UsageEvent[]>(`/v1/accounts/${account}/events`);
  }

  async voidRun(runId: Uuid): Promise<number> {
    const body = await this.#post<{ voided: number }>(`/v1/runs/${runId}/void`, undefined);
    return body.voided;
  }

  invoice(account: Uuid, start: string, end: string): Promise<Invoice> {
    const query = new URLSearchParams({ start, end }).toString();
    return this.#get<Invoice>(`/v1/accounts/${account}/invoice?${query}`);
  }

  /** Price token usage via the catalog, record the event, and charge credits — one idempotent call. */
  meterUsage(input: MeterUsageInput): Promise<UsageResult> {
    return this.#post<UsageResult>("/v1/usage", {
      org_id: input.orgId,
      account: input.account,
      model: input.model,
      idempotency_key: input.idempotencyKey,
      run_id: input.runId ?? null,
      usage: input.usage,
    });
  }

  #get<T>(path: string): Promise<T> {
    return this.#send<T>("GET", path, undefined);
  }

  #post<T>(path: string, body: unknown): Promise<T> {
    return this.#send<T>("POST", path, body);
  }

  async #send<T>(method: string, path: string, body: unknown): Promise<T> {
    const init: RequestInit = { method, headers: { "content-type": "application/json" } };
    if (body !== undefined) {
      init.body = JSON.stringify(body);
    }
    const response = await this.#fetch(`${this.#baseUrl}${path}`, init);
    return this.#parse<T>(response);
  }

  async #parse<T>(response: Response): Promise<T> {
    const text = await response.text();
    if (!response.ok) {
      throw toMeterError(response.status, text);
    }
    if (text.length === 0) {
      return undefined as T;
    }
    return JSON.parse(text) as T;
  }
}

function toMeterError(status: number, text: string): MeterError {
  const parsed = tryParseError(text);
  if (parsed === undefined) {
    return new MeterError(status, "error", text);
  }
  return new MeterError(status, parsed.error, parsed.message);
}

function tryParseError(text: string): EngineErrorBody | undefined {
  try {
    return JSON.parse(text) as EngineErrorBody;
  } catch {
    return undefined;
  }
}
