use std::{fmt::Display, sync::LazyLock};
use x402_rs::types::{PaymentRequiredResponse, PaymentRequirements, X402Version};

#[derive(Debug)]
pub struct X402Error(pub PaymentRequiredResponse);

impl From<PaymentRequiredResponse> for X402Error {
    fn from(value: PaymentRequiredResponse) -> Self {
        Self(value)
    }
}

impl Display for X402Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "402 Payment Required: {}", self.0)
    }
}

static ERR_PAYMENT_HEADER_REQUIRED: LazyLock<String> =
    LazyLock::new(|| "X-PAYMENT header is required".to_string());
static ERR_INVALID_PAYMENT_HEADER: LazyLock<String> =
    LazyLock::new(|| "Invalid or malformed payment header".to_string());
static ERR_NO_PAYMENT_MATCHING: LazyLock<String> =
    LazyLock::new(|| "Unable to find matching payment requirements".to_string());

/// Middleware application error with detailed context.
///
/// Encapsulates a `402 Payment Required` response that can be returned
/// when payment verification or settlement fails.
impl X402Error {
    pub fn payment_header_required(payment_requirements: Vec<PaymentRequirements>) -> Self {
        let payment_required_response = PaymentRequiredResponse {
            error: ERR_PAYMENT_HEADER_REQUIRED.clone(),
            accepts: payment_requirements,
            x402_version: X402Version::V1,
        };
        Self(payment_required_response)
    }

    pub fn invalid_payment_header(payment_requirements: Vec<PaymentRequirements>) -> Self {
        let payment_required_response = PaymentRequiredResponse {
            error: ERR_INVALID_PAYMENT_HEADER.clone(),
            accepts: payment_requirements,
            x402_version: X402Version::V1,
        };
        Self(payment_required_response)
    }

    pub fn no_payment_matching(payment_requirements: Vec<PaymentRequirements>) -> Self {
        let payment_required_response = PaymentRequiredResponse {
            error: ERR_NO_PAYMENT_MATCHING.clone(),
            accepts: payment_requirements,
            x402_version: X402Version::V1,
        };
        Self(payment_required_response)
    }

    pub fn verification_failed<E2: Display>(
        error: E2,
        payment_requirements: Vec<PaymentRequirements>,
    ) -> Self {
        let payment_required_response = PaymentRequiredResponse {
            error: format!("Verification Failed: {error}"),
            accepts: payment_requirements,
            x402_version: X402Version::V1,
        };
        Self(payment_required_response)
    }

    pub fn settlement_failed<E2: Display>(
        error: E2,
        payment_requirements: Vec<PaymentRequirements>,
    ) -> Self {
        let payment_required_response = PaymentRequiredResponse {
            error: format!("Settlement Failed: {error}"),
            accepts: payment_requirements,
            x402_version: X402Version::V1,
        };
        Self(payment_required_response)
    }
}

impl actix_web::ResponseError for X402Error {
    fn status_code(&self) -> actix_http::StatusCode {
        actix_http::StatusCode::PAYMENT_REQUIRED
    }
    fn error_response(&self) -> actix_web::HttpResponse<actix_web::body::BoxBody> {
        actix_web::HttpResponse::build(self.status_code()).json(&self.0)
    }
}
