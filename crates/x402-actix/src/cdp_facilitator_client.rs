use x402_rs::facilitator::Facilitator;

pub struct CdpFacilitatorClient {
    client: cdp_sdk::Client,
}

impl CdpFacilitatorClient {
    pub fn new(client: cdp_sdk::Client) -> Self {
        Self { client }
    }
}

impl Facilitator for CdpFacilitatorClient {
    type Error = cdp_sdk::Error<cdp_sdk::types::Error>;

    async fn verify(
        &self,
        request: &x402_rs::types::VerifyRequest,
    ) -> Result<x402_rs::types::VerifyResponse, Self::Error> {
        let resp = self
            .client
            .verify_x402_payment()
            .body(request)
            .send()
            .await?;
        Ok(resp.into_inner().into())
    }

    async fn settle(
        &self,
        request: &x402_rs::types::SettleRequest,
    ) -> Result<x402_rs::types::SettleResponse, Self::Error> {
        let resp = self
            .client
            .settle_x402_payment()
            .body(request)
            .send()
            .await?;
        resp.into_inner()
            .try_into()
            .map_err(|error: anyhow::Error| cdp_sdk::Error::Custom(error.to_string()))
    }

    async fn supported(
        &self,
    ) -> Result<x402_rs::types::SupportedPaymentKindsResponse, Self::Error> {
        let resp = self.client.supported_x402_payment_kinds().send().await?;
        Ok(resp.into_inner().into())
    }
}

#[cfg(test)]
mod tests {
    use cdp_sdk::{CDP_BASE_URL, Client, auth::WalletAuth};
    use reqwest_middleware::ClientBuilder;

    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_cdp_client() {
        let wallet_auth = WalletAuth::builder().build().unwrap();

        let http_client = ClientBuilder::new(reqwest::Client::new())
            .with(wallet_auth)
            .build();

        let client = Client::new_with_client(CDP_BASE_URL, http_client);

        let fac = CdpFacilitatorClient { client };
        let supported = fac.supported().await.unwrap();
        dbg!(supported);
    }
}
