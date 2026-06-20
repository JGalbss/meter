// Pure display helpers for rate-card pricing. Money never touches a float here: per-token prices are
// exact decimal strings from the engine, and we reformat them with string math only.

/**
 * Show an exact per-token decimal price as a per-million-tokens price, shifting the decimal point six
 * places right (×1,000,000) via string manipulation — no floating point near money.
 *
 * `"0.000003"` → `"$3"`, `"0.00000375"` → `"$3.75"`, `"0.0000003"` → `"$0.3"`, `"0"` → `"$0"`.
 */
export function pricePerMillionTokens(perToken: string): string {
  const negative = perToken.startsWith("-")
  const unsigned = negative ? perToken.slice(1) : perToken
  const [whole, fraction = ""] = unsigned.split(".")
  const shifted =
    fraction.length <= 6
      ? whole + fraction.padEnd(6, "0")
      : `${whole}${fraction.slice(0, 6)}.${fraction.slice(6)}`
  const [intPart, fracPart] = shifted.split(".")
  const cleanInt = intPart.replace(/^0+/, "") || "0"
  const trimmedFrac = fracPart === undefined ? "" : fracPart.replace(/0+$/, "")
  const magnitude =
    trimmedFrac.length === 0 ? cleanInt : `${cleanInt}.${trimmedFrac}`
  return `${negative ? "-" : ""}$${magnitude}`
}

/** True when a credit delta is exactly zero (the engine normalizes zero to `"0"`). */
export function isZeroDelta(delta: string): boolean {
  return /^-?0(\.0+)?$/.test(delta)
}

/** True when the proposed model is cheaper (a negative delta). */
export function isCheaper(delta: string): boolean {
  return delta.startsWith("-")
}

/** A human verdict for a current→proposed credit delta. */
export function deltaVerdict(delta: string): string {
  if (isZeroDelta(delta)) {
    return "Same cost on both models."
  }
  if (isCheaper(delta)) {
    return "The proposed model is cheaper for this usage."
  }
  return "The proposed model is more expensive for this usage."
}
