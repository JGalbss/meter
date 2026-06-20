import { describe, expect, it } from "vitest"

import {
  deltaVerdict,
  isCheaper,
  isZeroDelta,
  pricePerMillionTokens,
} from "./pricing-format"

describe("pricePerMillionTokens", () => {
  it("shifts an exact per-token decimal six places to a per-million price", () => {
    expect(pricePerMillionTokens("0.000003")).toBe("$3")
    expect(pricePerMillionTokens("0.000015")).toBe("$15")
    expect(pricePerMillionTokens("0.00000375")).toBe("$3.75")
    expect(pricePerMillionTokens("0.0000003")).toBe("$0.3")
  })

  it("handles zero and whole values", () => {
    expect(pricePerMillionTokens("0")).toBe("$0")
    expect(pricePerMillionTokens("0.000000")).toBe("$0")
    expect(pricePerMillionTokens("1")).toBe("$1000000")
  })

  it("preserves sign and trims trailing zeros", () => {
    expect(pricePerMillionTokens("-0.000003")).toBe("-$3")
    expect(pricePerMillionTokens("0.0000030")).toBe("$3")
  })

  it("never introduces floating-point error on long fractions", () => {
    // 0.000000123456 × 1e6 = 0.123456 exactly — no binary-float rounding.
    expect(pricePerMillionTokens("0.000000123456")).toBe("$0.123456")
  })
})

describe("credit delta helpers", () => {
  it("detects an exactly-zero delta", () => {
    expect(isZeroDelta("0")).toBe(true)
    expect(isZeroDelta("0.0")).toBe(true)
    expect(isZeroDelta("-0")).toBe(true)
    expect(isZeroDelta("0.5")).toBe(false)
    expect(isZeroDelta("-1")).toBe(false)
  })

  it("detects a cheaper (negative) delta", () => {
    expect(isCheaper("-2.5")).toBe(true)
    expect(isCheaper("2.5")).toBe(false)
    expect(isCheaper("0")).toBe(false)
  })

  it("renders a human verdict", () => {
    expect(deltaVerdict("0")).toBe("Same cost on both models.")
    expect(deltaVerdict("-1")).toBe(
      "The proposed model is cheaper for this usage."
    )
    expect(deltaVerdict("1")).toBe(
      "The proposed model is more expensive for this usage."
    )
  })
})
