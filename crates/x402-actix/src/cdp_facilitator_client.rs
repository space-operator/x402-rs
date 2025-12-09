use std::convert::Infallible;

use x402_rs::{facilitator::Facilitator, types::SupportedPaymentKind};

pub struct CdpFacilitatorClient {
    client: cdp_sdk::Client,
}

impl Facilitator for CdpFacilitatorClient {
    type Error = cdp_sdk::Error<cdp_sdk::types::Error>;

    async fn verify(
        &self,
        request: &x402_rs::types::VerifyRequest,
    ) -> Result<x402_rs::types::VerifyResponse, Self::Error> {
        todo!()
    }

    async fn settle(
        &self,
        request: &x402_rs::types::SettleRequest,
    ) -> Result<x402_rs::types::SettleResponse, Self::Error> {
        todo!()
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
