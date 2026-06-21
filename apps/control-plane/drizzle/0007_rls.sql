-- ADR 0007 tenant isolation, defense-in-depth: Row-Level Security on every org-scoped table.
--
-- App-level authorization (authorizeOrg / byIdInOrg) is the first line of defence; these policies are
-- the second, enforced by Postgres itself, so a bug or compromise in the app path cannot read or write
-- across tenants. The app connects as a non-superuser role (no BYPASSRLS) and sets `meter.org_id` per
-- request (SET LOCAL); platform / no-tenant operations set `meter.rls_bypass = 'on'`. With neither set
-- the predicate is NULL, so nothing is visible or writable (fail-closed). Superusers (e.g. the role
-- that runs migrations and admin tooling) bypass RLS, so this is transparent to provisioning.

-- Resolve the current tenant + bypass flag from per-request settings. STABLE so the planner can fold
-- them; `true` (missing_ok) makes an unset setting return NULL rather than erroring.
CREATE FUNCTION meter_current_org() RETURNS uuid LANGUAGE sql STABLE AS $$
  SELECT nullif(current_setting('meter.org_id', true), '')::uuid
$$;
--> statement-breakpoint
CREATE FUNCTION meter_rls_bypass() RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT current_setting('meter.rls_bypass', true) = 'on'
$$;
--> statement-breakpoint

-- organizations: the tenant key is the row's own id.
ALTER TABLE "organizations" ENABLE ROW LEVEL SECURITY;
--> statement-breakpoint
ALTER TABLE "organizations" FORCE ROW LEVEL SECURITY;
--> statement-breakpoint
CREATE POLICY "organizations_tenant" ON "organizations" FOR ALL
  USING (meter_rls_bypass() OR "id" = meter_current_org())
  WITH CHECK (meter_rls_bypass() OR "id" = meter_current_org());
--> statement-breakpoint

-- products
ALTER TABLE "products" ENABLE ROW LEVEL SECURITY;
--> statement-breakpoint
ALTER TABLE "products" FORCE ROW LEVEL SECURITY;
--> statement-breakpoint
CREATE POLICY "products_tenant" ON "products" FOR ALL
  USING (meter_rls_bypass() OR "org_id" = meter_current_org())
  WITH CHECK (meter_rls_bypass() OR "org_id" = meter_current_org());
--> statement-breakpoint

-- alert_rules
ALTER TABLE "alert_rules" ENABLE ROW LEVEL SECURITY;
--> statement-breakpoint
ALTER TABLE "alert_rules" FORCE ROW LEVEL SECURITY;
--> statement-breakpoint
CREATE POLICY "alert_rules_tenant" ON "alert_rules" FOR ALL
  USING (meter_rls_bypass() OR "org_id" = meter_current_org())
  WITH CHECK (meter_rls_bypass() OR "org_id" = meter_current_org());
--> statement-breakpoint

-- notifications
ALTER TABLE "notifications" ENABLE ROW LEVEL SECURITY;
--> statement-breakpoint
ALTER TABLE "notifications" FORCE ROW LEVEL SECURITY;
--> statement-breakpoint
CREATE POLICY "notifications_tenant" ON "notifications" FOR ALL
  USING (meter_rls_bypass() OR "org_id" = meter_current_org())
  WITH CHECK (meter_rls_bypass() OR "org_id" = meter_current_org());
--> statement-breakpoint

-- api_keys
ALTER TABLE "api_keys" ENABLE ROW LEVEL SECURITY;
--> statement-breakpoint
ALTER TABLE "api_keys" FORCE ROW LEVEL SECURITY;
--> statement-breakpoint
CREATE POLICY "api_keys_tenant" ON "api_keys" FOR ALL
  USING (meter_rls_bypass() OR "org_id" = meter_current_org())
  WITH CHECK (meter_rls_bypass() OR "org_id" = meter_current_org());
--> statement-breakpoint

-- webhooks
ALTER TABLE "webhooks" ENABLE ROW LEVEL SECURITY;
--> statement-breakpoint
ALTER TABLE "webhooks" FORCE ROW LEVEL SECURITY;
--> statement-breakpoint
CREATE POLICY "webhooks_tenant" ON "webhooks" FOR ALL
  USING (meter_rls_bypass() OR "org_id" = meter_current_org())
  WITH CHECK (meter_rls_bypass() OR "org_id" = meter_current_org());
--> statement-breakpoint

-- webhook_deliveries has no org_id; it inherits its tenant from the parent webhook. With the GUC set,
-- the subquery on webhooks is itself RLS-filtered to the caller's org, so this matches only deliveries
-- whose webhook belongs to the current tenant.
ALTER TABLE "webhook_deliveries" ENABLE ROW LEVEL SECURITY;
--> statement-breakpoint
ALTER TABLE "webhook_deliveries" FORCE ROW LEVEL SECURITY;
--> statement-breakpoint
CREATE POLICY "webhook_deliveries_tenant" ON "webhook_deliveries" FOR ALL
  USING (
    meter_rls_bypass() OR EXISTS (
      SELECT 1 FROM "webhooks" w
      WHERE w."id" = "webhook_deliveries"."webhook_id" AND w."org_id" = meter_current_org()
    )
  )
  WITH CHECK (
    meter_rls_bypass() OR EXISTS (
      SELECT 1 FROM "webhooks" w
      WHERE w."id" = "webhook_deliveries"."webhook_id" AND w."org_id" = meter_current_org()
    )
  );
