//! The enforcement service: reserve before, settle after, void on failure.

use meter_core::{AccountId, Money, RunId};
use meter_ledger::{
    LedgerBackend, LedgerEntry, LimitClass, ReservationId, ReserveOutcome, ReserveRequest,
    SettleRequest,
};
use meter_pricing::{price_usage, PricedUsage, RateCard, Usage};

use crate::error::EnforcementError;

/// The result of settling a reservation: the ledger entry and the pricing that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    pub entry: LedgerEntry,
    pub priced: PricedUsage,
}

/// Drives reserve/settle on a [`LedgerBackend`], pricing usage into credits via rate cards. Generic
/// over the backend so the in-memory reference and Postgres are interchangeable.
#[derive(Debug, Clone)]
pub struct EnforcementService<L> {
    ledger: L,
    credit_value: Money,
}

impl<L: LedgerBackend> EnforcementService<L> {
    /// Build a service over a ledger, with the cash value of one credit (e.g. `$0.02`).
    #[must_use]
    pub const fn new(ledger: L, credit_value: Money) -> Self {
        Self {
            ledger,
            credit_value,
        }
    }

    /// The underlying ledger, for reads and composition.
    #[must_use]
    pub const fn ledger(&self) -> &L {
        &self.ledger
    }

    /// Price a worst-case `estimate` into credits and place a durable hold before the call. For a HARD
    /// limit the returned [`ReserveOutcome::Denied`] means the call must not proceed.
    pub async fn reserve_usage(
        &self,
        account: AccountId,
        reservation_id: ReservationId,
        estimate: &Usage,
        card: &RateCard,
        limit: LimitClass,
        run_id: Option<RunId>,
    ) -> Result<ReserveOutcome, EnforcementError> {
        let priced = price_usage(estimate, card, &self.credit_value)?;
        let outcome = self
            .ledger
            .reserve(ReserveRequest {
                account,
                reservation_id,
                amount: priced.credits,
                limit,
                expires_at: None,
                run_id,
            })
            .await?;
        Ok(outcome)
    }

    /// Price the `actual` usage and post it, closing the reservation. Idempotent on `reservation_id`.
    pub async fn settle_usage(
        &self,
        reservation_id: ReservationId,
        actual: &Usage,
        card: &RateCard,
    ) -> Result<Settlement, EnforcementError> {
        let priced = price_usage(actual, card, &self.credit_value)?;
        let entry = self
            .ledger
            .settle(SettleRequest {
                reservation_id,
                actual: priced.credits,
            })
            .await?;
        Ok(Settlement { entry, priced })
    }

    /// Release a reservation without charging it (e.g. a failed or abandoned run). Idempotent.
    pub async fn void(&self, reservation_id: ReservationId) -> Result<(), EnforcementError> {
        self.ledger.void(reservation_id).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meter_core::RateCardId;
    use meter_core::{Credit, Currency, OrgId};
    use meter_ledger::{
        AccountScope, CreditSource, GrantRequest, InMemoryLedger, NewAccount, ReservationId,
    };
    use meter_pricing::{
        ChargeModel, ContextTier, Margin, Modality, PriceComponent, PricingDimension, RateCard,
        RateCardKind, Unit,
    };
    use rust_decimal_macros::dec;

    fn usd() -> Currency {
        Currency::new("USD").expect("currency")
    }

    fn card() -> RateCard {
        let token = |dimension, price| PriceComponent {
            dimension,
            modality: Modality::Text,
            context_tier: ContextTier::Standard,
            unit: Unit::Token,
            charge_model: ChargeModel::Standard,
            unit_price: Money::new(price, usd()),
        };
        RateCard {
            id: RateCardId::new(),
            kind: RateCardKind::ProviderCost,
            currency: usd(),
            version: 1,
            margin: Margin::NONE,
            components: vec![
                token(PricingDimension::InputUncached, dec!(0.000003)),
                token(PricingDimension::Output, dec!(0.000015)),
            ],
        }
    }

    fn usage(input: u32, output: u32) -> Usage {
        Usage::new(Modality::Text, ContextTier::Standard)
            .with(PricingDimension::InputUncached, input.into())
            .with(PricingDimension::Output, output.into())
    }

    async fn funded_account(
        service: &EnforcementService<InMemoryLedger>,
        credits: i64,
    ) -> AccountId {
        let account = service
            .ledger()
            .open_account(NewAccount {
                org_id: OrgId::new(),
                scope: AccountScope::Org,
                no_overdraft: true,
                parent_id: None,
            })
            .await
            .expect("open")
            .id;
        service
            .ledger()
            .grant(GrantRequest {
                account,
                amount: Credit::from(credits),
                source: CreditSource::Paid,
                idempotency_key: None,
            })
            .await
            .expect("grant");
        account
    }

    #[tokio::test]
    async fn reserves_then_settles_actual() {
        // 1 credit = 1 micro-dollar, so credits == cost * 1_000_000.
        let service =
            EnforcementService::new(InMemoryLedger::new(), Money::new(dec!(0.000001), usd()));
        let account = funded_account(&service, 20_000).await;
        let reservation = ReservationId::new();

        // estimate 1000 in + 500 out -> cost 0.0105 -> 10_500 credits held.
        let outcome = service
            .reserve_usage(
                account,
                reservation,
                &usage(1000, 500),
                &card(),
                LimitClass::Hard,
                None,
            )
            .await
            .expect("reserve");
        assert!(matches!(outcome, ReserveOutcome::Allowed { .. }));
        assert_eq!(
            service.ledger().balance(account).await.unwrap().held,
            Credit::from(10_500_i64)
        );

        // actual 800 in + 400 out -> cost 0.0084 -> 8_400 credits charged.
        let settlement = service
            .settle_usage(reservation, &usage(800, 400), &card())
            .await
            .expect("settle");
        assert_eq!(settlement.priced.credits, Credit::from(8_400_i64));
        let balance = service.ledger().balance(account).await.unwrap();
        assert_eq!(balance.settled, Credit::from(11_600_i64)); // 20_000 - 8_400
        assert_eq!(balance.held, Credit::ZERO);
    }

    #[tokio::test]
    async fn denies_when_estimate_exceeds_balance() {
        let service =
            EnforcementService::new(InMemoryLedger::new(), Money::new(dec!(0.000001), usd()));
        let account = funded_account(&service, 1_000).await; // only 1_000 credits
        let outcome = service
            .reserve_usage(
                account,
                ReservationId::new(),
                &usage(1000, 500),
                &card(),
                LimitClass::Hard,
                None,
            )
            .await
            .expect("reserve");
        assert!(matches!(outcome, ReserveOutcome::Denied { .. }));
    }

    #[tokio::test]
    async fn void_releases_a_failed_run() {
        let service =
            EnforcementService::new(InMemoryLedger::new(), Money::new(dec!(0.000001), usd()));
        let account = funded_account(&service, 20_000).await;
        let reservation = ReservationId::new();
        service
            .reserve_usage(
                account,
                reservation,
                &usage(1000, 500),
                &card(),
                LimitClass::Hard,
                None,
            )
            .await
            .expect("reserve");
        service.void(reservation).await.expect("void");
        assert_eq!(
            service.ledger().balance(account).await.unwrap().available(),
            Credit::from(20_000_i64)
        );
    }
}
