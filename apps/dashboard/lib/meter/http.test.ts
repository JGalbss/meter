import { afterEach, describe, expect, it, vi } from "vitest"

import {
  describeError,
  getResult,
  isTimeoutError,
  requestOrThrow,
} from "./http"

afterEach(() => {
  vi.unstubAllGlobals()
})

function stubFetch(impl: () => Promise<Response>): void {
  vi.stubGlobal("fetch", vi.fn(impl))
}

describe("isTimeoutError", () => {
  it("recognises an AbortSignal.timeout abort", () => {
    expect(isTimeoutError(new DOMException("timed out", "TimeoutError"))).toBe(
      true
    )
  })

  it("rejects other errors", () => {
    expect(isTimeoutError(new Error("boom"))).toBe(false)
    expect(isTimeoutError(new DOMException("aborted", "AbortError"))).toBe(
      false
    )
    expect(isTimeoutError("nope")).toBe(false)
  })
})

describe("describeError", () => {
  it("labels a timeout distinctly", () => {
    expect(describeError("engine", new DOMException("x", "TimeoutError"))).toBe(
      "engine timed out"
    )
  })

  it("passes through an Error message", () => {
    expect(describeError("engine", new Error("connection refused"))).toBe(
      "connection refused"
    )
  })

  it("falls back to unreachable for non-Errors", () => {
    expect(describeError("control plane", { weird: true })).toBe(
      "control plane unreachable"
    )
  })
})

describe("getResult", () => {
  it("returns the decoded body on success", async () => {
    stubFetch(() =>
      Promise.resolve(new Response(JSON.stringify([1, 2, 3]), { status: 200 }))
    )
    const result = await getResult<number[]>("engine", "http://x/usage")
    expect(result).toEqual({ ok: true, data: [1, 2, 3] })
  })

  it("maps a non-2xx to a labelled error", async () => {
    stubFetch(() => Promise.resolve(new Response("", { status: 503 })))
    const result = await getResult("engine", "http://x/usage")
    expect(result).toEqual({ ok: false, error: "engine responded 503" })
  })

  it("maps a transport failure to the error message", async () => {
    stubFetch(() => Promise.reject(new Error("ECONNREFUSED")))
    const result = await getResult("control plane", "http://x/orgs")
    expect(result).toEqual({ ok: false, error: "ECONNREFUSED" })
  })

  it("maps a timeout to a labelled timeout error", async () => {
    stubFetch(() =>
      Promise.reject(new DOMException("timed out", "TimeoutError"))
    )
    const result = await getResult("engine", "http://x/usage")
    expect(result).toEqual({ ok: false, error: "engine timed out" })
  })

  it("passes a timeout signal to fetch", async () => {
    const spy = vi.fn<(input: string, init?: RequestInit) => Promise<Response>>(
      () => Promise.resolve(new Response("{}", { status: 200 }))
    )
    vi.stubGlobal("fetch", spy)
    await getResult("engine", "http://x/usage")
    const init = spy.mock.calls[0]?.[1]
    expect(init?.signal).toBeInstanceOf(AbortSignal)
    expect(init?.cache).toBe("no-store")
  })
})

describe("requestOrThrow", () => {
  it("returns the response on success", async () => {
    stubFetch(() => Promise.resolve(new Response("{}", { status: 201 })))
    const response = await requestOrThrow("control plane", "http://x/orgs", {
      method: "POST",
    })
    expect(response.status).toBe(201)
  })

  it("throws a labelled error on a non-2xx", async () => {
    stubFetch(() => Promise.resolve(new Response("", { status: 404 })))
    await expect(
      requestOrThrow("control plane", "http://x/orgs")
    ).rejects.toThrow("control plane responded 404")
  })
})
