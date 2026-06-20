//! SDK error type.

/** An error returned by the meter engine (carries the HTTP status and the engine's error code). */
export class MeterError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "MeterError";
    this.status = status;
    this.code = code;
  }
}
