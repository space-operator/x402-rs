use actix_http::header::HeaderMap;
use serde_json::json;
use x402_rs::{facilitator::Facilitator, types::{Base64Bytes, FacilitatorErrorReason, PaymentPayload, PaymentRequiredResponse, PaymentRequirements, SettleRequest, SettleResponse, VerifyRequest, VerifyResponse, X402Version}};
use std::sync::Arc;

use crate::error::X402Error;

/// A service-level helper struct responsible for verifying and settling
/// x402 payments based on request headers and known payment requirements.
pub struct X402Paygate<F> {
    pub facilitator: Arc<F>,
    pub payment_requirements: Arc<Vec<PaymentRequirements>>,
    pub settle_before_execution: bool,
}

impl<F> X402Paygate<F>
where
    F: Facilitator,
{
    /// Parses the `X-Payment` header and returns a decoded [`PaymentPayload`], or constructs a 402 error if missing or malformed as [`X402Error`].
    pub async fn extract_payment_payload(
        &self,
        headers: &HeaderMap,
    ) -> Result<PaymentPayload, X402Error> {
        let payment_header = headers.get("X-Payment");
        let supported = self.facilitator.supported().await.map_err(|e| {
            X402Error(PaymentRequiredResponse {
                x402_version: X402Version::V1,
                error: format!("Unable to retrieve supported payment schemes: {e}"),
                accepts: vec![],
            })
        })?;
        match payment_header {
            None => {
                let requirements = self
                    .payment_requirements
                    .as_ref()
                    .iter()
                    .map(|r| {
                        let mut r = r.clone();
                        let network = r.network;
                        let extra = supported
                            .kinds
                            .iter()
                            .find(|s| s.network == network.to_string())
                            .cloned()
                            .and_then(|s| s.extra);
                        if let Some(extra) = extra {
                            r.extra = Some(json!({
                                "feePayer": extra.fee_payer
                            }));
                            r
                        } else {
                            r
                        }
                    })
                    .collect::<Vec<_>>();
                Err(X402Error::payment_header_required(requirements))
            }
            Some(payment_header) => {
                let base64 = Base64Bytes::from(payment_header.as_bytes());
                let payment_payload = PaymentPayload::try_from(base64);
                match payment_payload {
                    Ok(payment_payload) => Ok(payment_payload),
                    Err(_) => Err(X402Error::invalid_payment_header(
                        self.payment_requirements.as_ref().clone(),
                    )),
                }
            }
        }
    }

    /// Finds the payment requirement entry matching the given payload's scheme and network.
    fn find_matching_payment_requirements(
        &self,
        payment_payload: &PaymentPayload,
    ) -> Option<PaymentRequirements> {
        self.payment_requirements
            .iter()
            .find(|requirement| {
                requirement.scheme == payment_payload.scheme
                    && requirement.network == payment_payload.network
            })
            .cloned()
    }

    /// Verifies the provided payment using the facilitator and known requirements. Returns a [`VerifyRequest`] if the payment is valid.
    pub async fn verify_payment(
        &self,
        payment_payload: PaymentPayload,
    ) -> Result<VerifyRequest, X402Error> {
        let selected = self
            .find_matching_payment_requirements(&payment_payload)
            .ok_or(X402Error::no_payment_matching(
                self.payment_requirements.as_ref().clone(),
            ))?;
        let verify_request = VerifyRequest {
            x402_version: payment_payload.x402_version,
            payment_payload,
            payment_requirements: selected,
        };
        let verify_response = self
            .facilitator
            .verify(&verify_request)
            .await
            .map_err(|e| {
                X402Error::verification_failed(e, self.payment_requirements.as_ref().clone())
            })?;
        match verify_response {
            VerifyResponse::Valid { .. } => Ok(verify_request),
            VerifyResponse::Invalid { reason, .. } => Err(X402Error::verification_failed(
                reason,
                self.payment_requirements.as_ref().clone(),
            )),
        }
    }

    /// Attempts to settle a verified payment on-chain. Returns [`SettleResponse`] on success or emits a 402 error.
    pub async fn settle_payment(
        &self,
        settle_request: &SettleRequest,
    ) -> Result<SettleResponse, X402Error> {
        let settlement = self.facilitator.settle(settle_request).await.map_err(|e| {
            X402Error::settlement_failed(e, self.payment_requirements.as_ref().clone())
        })?;
        if settlement.success {
            Ok(settlement)
        } else {
            let error_reason = settlement
                .error_reason
                .unwrap_or(FacilitatorErrorReason::InvalidScheme);
            Err(X402Error::settlement_failed(
                error_reason,
                self.payment_requirements.as_ref().clone(),
            ))
        }
    }
}
