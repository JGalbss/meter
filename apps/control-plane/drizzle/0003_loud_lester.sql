ALTER TABLE "alert_rules" ADD COLUMN "account_id" uuid;--> statement-breakpoint
ALTER TABLE "alert_rules" ADD COLUMN "credit_limit" numeric;--> statement-breakpoint
ALTER TABLE "alert_rules" ADD COLUMN "window_days" integer DEFAULT 30 NOT NULL;--> statement-breakpoint
ALTER TABLE "alert_rules" ADD COLUMN "last_status" text;