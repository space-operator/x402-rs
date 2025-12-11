//! Core trait defining the verification and settlement interface for x402 facilitators.
//!
//! Implementors of this trait are responsible for validating incoming payment payloads
//! against specified requirements [`Facilitator::verify`] and executing on-chain transfers [`Facilitator::settle`].

use either::Either;

use crate::types::{
    SettleRequest, SettleResponse, SupportedPaymentKindsResponse, VerifyRequest, VerifyResponse,
};
use std::fmt::{Debug, Display};
use std::sync::Arc;

/// Trait defining the asynchronous interface for x402 payment facilitators.
///
/// This interface is implemented by any type that performs validation and
/// settlement of payment payloads according to the x402 specification.
pub trait Facilitator {
    /// The error type returned by this facilitator.
    type Error: Debug + Display;

    /// Verifies a proposed x402 payment payload against a [`VerifyRequest`].
    ///
    /// This includes checking payload integrity, signature validity, balance sufficiency,
    /// network compatibility, and compliance with the declared payment requirements.
    ///
    /// # Returns
    ///
    /// A [`VerifyResponse`] indicating success or failure, wrapped in a [`Result`].
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] if any validation step fails.
    fn verify(
        &self,
        request: &VerifyRequest,
    ) -> impl Future<Output = Result<VerifyResponse, Self::Error>> + Send;

    /// Executes an on-chain x402 settlement for a valid [`SettleRequest`].
    ///
    /// This method should re-validate the payment and, if valid, perform
    /// an onchain call to settle the payment.
    ///
    /// # Returns
    ///
    /// A [`SettleResponse`] indicating whether the settlement was successful, and
    /// containing any on-chain transaction metadata.
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] if verification or settlement fails.
    fn settle(
        &self,
        request: &SettleRequest,
    ) -> impl Future<Output = Result<SettleResponse, Self::Error>> + Send;

    #[allow(dead_code)] // For some reason clippy believes it is not used.
    fn supported(
        &self,
    ) -> impl Future<Output = Result<SupportedPaymentKindsResponse, Self::Error>> + Send;
}

impl<T: Facilitator> Facilitator for Arc<T> {
    type Error = T::Error;

    fn verify(
        &self,
        request: &VerifyRequest,
    ) -> impl Future<Output = Result<VerifyResponse, Self::Error>> + Send {
        self.as_ref().verify(request)
    }

    fn settle(
        &self,
        request: &SettleRequest,
    ) -> impl Future<Output = Result<SettleResponse, Self::Error>> + Send {
        self.as_ref().settle(request)
    }

    fn supported(
        &self,
    ) -> impl Future<Output = Result<SupportedPaymentKindsResponse, Self::Error>> + Send {
        self.as_ref().supported()
    }
}

impl<A, B> Facilitator for Either<A, B>
where
    A: Facilitator + Sync,
    B: Facilitator + Sync,
{
    type Error = Either<A::Error, B::Error>;

    async fn verify(&self, request: &VerifyRequest) -> Result<VerifyResponse, Self::Error> {
        match self {
            Either::Left(a) => a.verify(request).await.map_err(Either::Left),
            Either::Right(b) => b.verify(request).await.map_err(Either::Right),
        }
    }

    async fn settle(&self, request: &SettleRequest) -> Result<SettleResponse, Self::Error> {
        match self {
            Either::Left(a) => a.settle(request).await.map_err(Either::Left),
            Either::Right(b) => b.settle(request).await.map_err(Either::Right),
        }
    }

    async fn supported(&self) -> Result<SupportedPaymentKindsResponse, Self::Error> {
        match self {
            Either::Left(a) => a.supported().await.map_err(Either::Left),
            Either::Right(b) => b.supported().await.map_err(Either::Right),
        }
    }
}
