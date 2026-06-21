-- ADR 0007 tenant isolation: add the api-key scope.
-- Backfill existing keys to 'platform' so they keep their current cross-org access (non-breaking),
-- then set the column default to 'org' so newly minted keys are tenant-scoped by default.
ALTER TABLE "api_keys" ADD COLUMN "scope" text DEFAULT 'platform' NOT NULL;
--> statement-breakpoint
ALTER TABLE "api_keys" ALTER COLUMN "scope" SET DEFAULT 'org';
