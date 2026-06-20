//! Webhook signing. Every delivery carries `X-Meter-Signature: sha256=<hex>` — an HMAC-SHA256 of the
//! raw request body keyed by the endpoint's secret. Receivers recompute and compare to authenticate.

import { createHmac, timingSafeEqual } from "node:crypto";

/** Compute the signature header value for a raw body. */
export function signPayload(secret: string, body: string): string {
  const hex = createHmac("sha256", secret).update(body).digest("hex");
  return `sha256=${hex}`;
}

/** Constant-time check that a signature matches the body under the secret. */
export function isValidSignature(secret: string, body: string, signature: string): boolean {
  const expected = Buffer.from(signPayload(secret, body));
  const provided = Buffer.from(signature);
  if (expected.length !== provided.length) {
    return false;
  }
  return timingSafeEqual(expected, provided);
}
