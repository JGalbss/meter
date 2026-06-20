//! HTTP router: the control-plane CRUD surface the dashboard hits. Config and operational state only —
//! money lives in the engine. Routes are composed from one module per resource. No money is computed
//! here.

import { HttpRouter, HttpServerResponse } from "@effect/platform";
import { Effect } from "effect";

import type { Database } from "../db/service";
import { alertRoutes } from "./routes/alerts";
import { notificationRoutes } from "./routes/notifications";
import { organizationRoutes } from "./routes/organizations";
import { productRoutes } from "./routes/products";

export const router: HttpRouter.HttpRouter<never, Database> = HttpRouter.empty.pipe(
  HttpRouter.get("/health", Effect.succeed(HttpServerResponse.unsafeJson({ status: "ok" }))),
  organizationRoutes,
  productRoutes,
  notificationRoutes,
  alertRoutes,
);
