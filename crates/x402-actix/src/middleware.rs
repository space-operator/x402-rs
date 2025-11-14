use std::{collections::HashSet, sync::Arc};

use actix_http::Uri;
use serde_json::json;
use url::Url;
use x402_rs::{
    network::Network,
    types::{MixedAddress, PaymentRequirements, Scheme, TokenAmount},
};

use crate::{
    facilitator_client::{FacilitatorClient, FacilitatorClientError},
    paygate::X402Paygate,
    price::PriceTag,
};

/// A variant of [`PaymentRequirements`] without the `resource` field.
/// This allows resources to be dynamically inferred per request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRequirementsNoResource {
    pub scheme: Scheme,
    pub network: Network,
    pub max_amount_required: TokenAmount,
    // no resource: Url,
    pub description: String,
    pub mime_type: String,
    pub pay_to: MixedAddress,
    pub max_timeout_seconds: u64,
    pub asset: MixedAddress,
    pub extra: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
}

impl PaymentRequirementsNoResource {
    /// Converts this partial requirement into a full [`PaymentRequirements`]
    /// using the provided resource URL.
    pub fn to_payment_requirements(&self, resource: Url) -> PaymentRequirements {
        PaymentRequirements {
            scheme: self.scheme,
            network: self.network,
            max_amount_required: self.max_amount_required,
            resource,
            description: self.description.clone(),
            mime_type: self.mime_type.clone(),
            pay_to: self.pay_to.clone(),
            max_timeout_seconds: self.max_timeout_seconds,
            asset: self.asset.clone(),
            extra: self.extra.clone(),
            output_schema: self.output_schema.clone(),
        }
    }
}

/// Enum capturing either fully constructed [`PaymentRequirements`] (with `resource`)
/// or resource-less variants that must be completed at runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaymentOffers {
    /// [`PaymentRequirements`] with static `resource` field.
    Ready(Arc<Vec<PaymentRequirements>>),
    /// [`PaymentRequirements`] lacking `resource`, to be added per request.
    NoResource {
        partial: Vec<PaymentRequirementsNoResource>,
        base_url: Url,
    },
}

/// Middleware layer that enforces x402 payment verification and settlement.
///
/// Wraps an Axum service, intercepts incoming HTTP requests, verifies the payment
/// using the configured facilitator, and performs settlement after a successful response.
/// Adds a `X-Payment-Response` header to the final HTTP response.
#[derive(Clone, Debug)]
pub struct X402Middleware<F> {
    /// The facilitator used to verify and settle payments.
    facilitator: Arc<F>,
    /// Optional description string passed along with payment requirements. Empty string by default.
    description: Option<String>,
    /// Optional MIME type of the protected resource. `application/json` by default.
    mime_type: Option<String>,
    /// Optional resource URL. If not set, it will be derived from a request URI.
    resource: Option<Url>,
    /// Optional base URL for computing full resource URLs if `resource` is not set, see [`X402Middleware::resource`].
    base_url: Option<Url>,
    /// List of price tags accepted for this endpoint.
    price_tag: Vec<PriceTag>,
    /// Timeout in seconds for payment settlement.
    max_timeout_seconds: u64,
    /// Optional input schema describing the API endpoint's input specification.
    input_schema: Option<serde_json::Value>,
    /// Optional output schema describing the API endpoint's output specification.
    output_schema: Option<serde_json::Value>,
    /// Whether to settle payment before executing the request (true) or after (false, default).
    settle_before_execution: bool,
    /// Cached set of payment offers for this middleware instance.
    ///
    /// This field holds either:
    /// - a fully constructed list of [`PaymentRequirements`] (if [`X402Middleware::with_resource`] was used),
    /// - or a partial list without `resource`, in which case the resource URL will be computed dynamically per request.
    ///   In this case, please add `base_url` via [`X402Middleware::with_base_url`].
    payment_offers: Arc<PaymentOffers>,
}

impl<F> X402Middleware<F> {
    /// Creates a new middleware instance with a default configuration.
    pub async fn new(facilitator: F) -> Self {
        Self {
            facilitator: Arc::new(facilitator),
            description: None,
            mime_type: None,
            resource: None,
            base_url: None,
            max_timeout_seconds: 300,
            price_tag: Vec::new(),
            input_schema: None,
            output_schema: None,
            settle_before_execution: false,
            payment_offers: Arc::new(PaymentOffers::Ready(Arc::new(Vec::new()))),
        }
    }

    /// Returns the configured base URL for x402-protected resources, or `http://localhost/` if not set.
    pub fn base_url(&self) -> Url {
        self.base_url
            .clone()
            .unwrap_or(Url::parse("http://localhost/").unwrap())
    }
}

fn gather_payment_requirements(
    payment_offers: &PaymentOffers,
    req_uri: &Uri,
) -> Arc<Vec<PaymentRequirements>> {
    match payment_offers {
        PaymentOffers::Ready(requirements) => {
            // requirements is &Arc<Vec<PaymentRequirements>>
            Arc::clone(requirements)
        }
        PaymentOffers::NoResource { partial, base_url } => {
            let resource = {
                let mut resource_url = base_url.clone();
                resource_url.set_path(req_uri.path());
                resource_url.set_query(req_uri.query());
                resource_url
            };
            let payment_requirements = partial
                .iter()
                .map(|partial| partial.to_payment_requirements(resource.clone()))
                .collect::<Vec<_>>();
            Arc::new(payment_requirements)
        }
    }
}

impl<F> X402Middleware<F>
where
    F: Clone,
{
    pub fn to_paygate(&self, uri: &Uri) -> X402Paygate<F> {
        let payment_requirements = gather_payment_requirements(&self.payment_offers, uri);
        X402Paygate {
            facilitator: self.facilitator.clone(),
            payment_requirements,
        }
    }

    /// Sets the description field on all generated payment requirements.
    pub fn with_description(&self, description: &str) -> Self {
        let mut this = self.clone();
        this.description = Some(description.to_string());
        this.recompute_offers()
    }

    /// Sets the MIME type of the protected resource.
    /// This is exposed as a part of [`PaymentRequirements`] passed to the client.
    pub fn with_mime_type(&self, mime: &str) -> Self {
        let mut this = self.clone();
        this.mime_type = Some(mime.to_string());
        this.recompute_offers()
    }

    /// Sets the resource URL directly, avoiding fragile auto-detection from the request.
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn with_resource(&self, resource: Url) -> Self {
        let mut this = self.clone();
        this.resource = Some(resource);
        this.recompute_offers()
    }

    /// Sets the base URL used to construct resource URLs dynamically.
    ///
    /// Note: If [`with_resource`] is not called, this base URL is combined with
    /// each request's path/query to compute the resource. If not set, defaults to `http://localhost/`.
    ///
    /// ⚠️ In production, prefer calling `with_resource` or setting a precise `base_url` to avoid accidental localhost fallback.
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn with_base_url(&self, base_url: Url) -> Self {
        let mut this = self.clone();
        this.base_url = Some(base_url);
        this.recompute_offers()
    }

    /// Sets the maximum allowed payment timeout, in seconds.
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn with_max_timeout_seconds(&self, seconds: u64) -> Self {
        let mut this = self.clone();
        this.max_timeout_seconds = seconds;
        this.recompute_offers()
    }

    /// Replaces all price tags with the provided value(s).
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn with_price_tag<T: Into<Vec<PriceTag>>>(&self, price_tag: T) -> Self {
        let mut this = self.clone();
        this.price_tag = price_tag.into();
        this.recompute_offers()
    }

    /// Adds new price tags to the existing list, avoiding duplicates.
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn or_price_tag<T: Into<Vec<PriceTag>>>(&self, price_tag: T) -> Self {
        let mut this = self.clone();
        let mut seen: HashSet<PriceTag> = this.price_tag.iter().cloned().collect();
        for tag in price_tag.into() {
            if seen.insert(tag.clone()) {
                this.price_tag.push(tag);
            }
        }
        this.recompute_offers()
    }

    /// Sets the input schema describing the API endpoint's expected inputs.
    ///
    /// The input schema will be embedded in `PaymentRequirements.outputSchema.input`.
    /// This can include information about HTTP method, query parameters, headers, body schema, etc.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use serde_json::json;
    ///
    /// let input_schema = json!({
    ///     "type": "http",
    ///     "method": "GET",
    ///     "discoverable": true,
    ///     "queryParams": {
    ///         "location": {
    ///             "type": "string",
    ///             "description": "City name",
    ///             "required": true
    ///         }
    ///     }
    /// });
    ///
    /// x402.with_input_schema(input_schema)
    /// ```
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn with_input_schema(&self, schema: serde_json::Value) -> Self {
        let mut this = self.clone();
        this.input_schema = Some(schema);
        this.recompute_offers()
    }

    /// Sets the output schema describing the API endpoint's response format.
    ///
    /// The output schema will be embedded in `PaymentRequirements.outputSchema.output`.
    /// This can include information about the response structure, content type, etc.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use serde_json::json;
    ///
    /// let output_schema = json!({
    ///     "type": "object",
    ///     "properties": {
    ///         "temperature": { "type": "number" },
    ///         "conditions": { "type": "string" }
    ///     }
    /// });
    ///
    /// x402.with_output_schema(output_schema)
    /// ```
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn with_output_schema(&self, schema: serde_json::Value) -> Self {
        let mut this = self.clone();
        this.output_schema = Some(schema);
        this.recompute_offers()
    }

    /// Enables settlement prior to request execution.
    ///
    /// When enabled, the payment will be settled on-chain **before** the protected
    /// request handler is invoked. This prevents issues where:
    /// - Failed settlements need to be retried via an external process
    /// - Payment authorization expires before final settlement
    ///
    /// When disabled (default), settlement occurs after successful request execution.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use x402_actix::middleware::X402Middleware;
    /// use x402_rs::network::{Network, USDCDeployment};
    /// use x402_actix::price::IntoPriceTag;
    ///
    /// let x402 = X402Middleware::try_from("https://facilitator.example.com/")
    ///     .unwrap()
    ///     .settle_before_execution()
    ///     .with_price_tag(
    ///     );
    /// ```
    #[allow(dead_code)] // Public for consumption by downstream crates.
    pub fn settle_before_execution(&self) -> Self {
        let mut this = self.clone();
        this.settle_before_execution = true;
        this
    }

    fn recompute_offers(mut self) -> Self {
        let base_url = self.base_url();
        let description = self.description.clone().unwrap_or_default();
        let mime_type = self
            .mime_type
            .clone()
            .unwrap_or("application/json".to_string());
        let max_timeout_seconds = self.max_timeout_seconds;

        // Construct the complete output_schema from input and output schemas
        let complete_output_schema = match (&self.input_schema, &self.output_schema) {
            (Some(input), Some(output)) => Some(json!({
                "input": input,
                "output": output
            })),
            (Some(input), None) => Some(json!({
                "input": input
            })),
            (None, Some(output)) => Some(json!({
                "output": output
            })),
            (None, None) => None,
        };

        let no_resource = self.price_tag.iter().map(|price_tag| {
            let extra = if let Some(eip712) = price_tag.token.eip712.clone() {
                Some(json!({
                    "name": eip712.name,
                    "version": eip712.version
                }))
            } else if matches!(
                price_tag.token.network(),
                Network::SolanaDevnet | Network::Solana
            ) {
                None
            } else {
                None
            };
            PaymentRequirementsNoResource {
                scheme: Scheme::Exact,
                network: price_tag.token.network(),
                max_amount_required: price_tag.amount,
                description: description.clone(),
                mime_type: mime_type.clone(),
                pay_to: price_tag.pay_to.clone(),
                max_timeout_seconds,
                asset: price_tag.token.address(),
                extra,
                output_schema: complete_output_schema.clone(),
            }
        });

        let payment_offers = match self.resource.clone() {
            None => PaymentOffers::NoResource {
                partial: no_resource.collect(),
                base_url,
            },
            Some(resource) => {
                let payment_requirements = no_resource
                    .map(|r| r.to_payment_requirements(resource.clone()))
                    .collect();
                PaymentOffers::Ready(Arc::new(payment_requirements))
            }
        };
        self.payment_offers = Arc::new(payment_offers);
        self
    }
}

impl X402Middleware<FacilitatorClient> {
    pub fn facilitator_url(&self) -> &Url {
        self.facilitator.base_url()
    }
}
