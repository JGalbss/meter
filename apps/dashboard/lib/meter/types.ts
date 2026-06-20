//! Response shapes from the meter control plane. Mirrors `apps/control-plane` repositories.

export interface Organization {
  readonly id: string;
  readonly slug: string;
  readonly name: string;
  readonly defaultCurrency: string;
}

export interface Product {
  readonly id: string;
  readonly orgId: string;
  readonly key: string;
  readonly name: string;
}

export interface Notification {
  readonly id: string;
  readonly orgId: string;
  readonly type: string;
  readonly severity: string;
  readonly title: string;
  readonly body: string;
  readonly data: unknown;
  readonly status: string;
  readonly createdAt: string;
  readonly readAt: string | null;
  readonly ackedAt: string | null;
}

export interface AlertRule {
  readonly id: string;
  readonly orgId: string;
  readonly name: string;
  readonly scope: string;
  readonly metric: string;
  readonly threshold: string;
  readonly action: string;
  readonly enabled: boolean;
  readonly accountId: string | null;
  readonly creditLimit: string | null;
  readonly windowDays: number;
  readonly lastStatus: string | null;
  readonly createdAt: string;
}

export interface Webhook {
  readonly id: string;
  readonly orgId: string;
  readonly url: string;
  readonly eventTypes: readonly string[];
  readonly enabled: boolean;
  readonly createdAt: string;
}

export interface ApiKey {
  readonly id: string;
  readonly orgId: string;
  readonly name: string;
  readonly prefix: string;
  readonly createdAt: string;
  readonly lastUsedAt: string | null;
  readonly revokedAt: string | null;
}

export interface CreatedApiKey extends ApiKey {
  readonly token: string;
}

export interface WebhookDelivery {
  readonly id: string;
  readonly webhookId: string;
  readonly notificationId: string | null;
  readonly event: string;
  readonly payload: unknown;
  readonly status: string;
  readonly responseStatus: number | null;
  readonly error: string | null;
  readonly attempts: number;
  readonly createdAt: string;
}
