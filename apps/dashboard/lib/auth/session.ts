//! Dashboard session auth. Next.js server actions are public HTTP endpoints, so every action — and
//! the dashboard layout — must verify a session before touching data. Sessions are a signed
//! (HMAC-SHA256), expiring cookie; login checks a shared admin password. Configure with
//! `DASHBOARD_SESSION_SECRET` and `DASHBOARD_PASSWORD`; with neither set, the dashboard is locked.

import { createHmac, timingSafeEqual } from "node:crypto";

import { cookies } from "next/headers";

const COOKIE = "meter_dash_session";
const MAX_AGE_SECONDS = 60 * 60 * 12; // 12 hours

function secret(): string {
  return process.env.DASHBOARD_SESSION_SECRET ?? "";
}

function sign(payload: string): string {
  return createHmac("sha256", secret()).update(payload).digest("hex");
}

function constantTimeEquals(a: string, b: string): boolean {
  const left = Buffer.from(a);
  const right = Buffer.from(b);
  if (left.length !== right.length) {
    return false;
  }
  return timingSafeEqual(left, right);
}

function sessionValue(nowMs: number): string {
  const exp = Math.floor(nowMs / 1000) + MAX_AGE_SECONDS;
  const payload = `v1.${exp}`;
  return `${payload}.${sign(payload)}`;
}

function isValid(value: string | undefined, nowMs: number): boolean {
  if (value === undefined || secret().length === 0) {
    return false;
  }
  const parts = value.split(".");
  if (parts.length !== 3) {
    return false;
  }
  const payload = `${parts[0]}.${parts[1]}`;
  if (!constantTimeEquals(sign(payload), parts[2])) {
    return false;
  }
  const exp = Number(parts[1]);
  if (!Number.isFinite(exp) || exp * 1000 < nowMs) {
    return false;
  }
  return true;
}

/** Whether the request carries a valid, unexpired session. */
export async function hasValidSession(): Promise<boolean> {
  const store = await cookies();
  return isValid(store.get(COOKIE)?.value, Date.now());
}

/** Throw if the request is not authenticated — call first in every server action. */
export async function requireSession(): Promise<void> {
  if (!(await hasValidSession())) {
    throw new Error("unauthorized");
  }
}

/** Verify the submitted admin password (constant-time). */
export function verifyPassword(input: string): boolean {
  const expected = process.env.DASHBOARD_PASSWORD ?? "";
  if (expected.length === 0) {
    return false;
  }
  return constantTimeEquals(input, expected);
}

/** Start a session (after a successful password check). */
export async function startSession(): Promise<void> {
  const store = await cookies();
  store.set(COOKIE, sessionValue(Date.now()), {
    httpOnly: true,
    sameSite: "lax",
    secure: true,
    path: "/",
    maxAge: MAX_AGE_SECONDS,
  });
}

/** End the session. */
export async function endSession(): Promise<void> {
  const store = await cookies();
  store.delete(COOKIE);
}
