use actix_web::{App, HttpServer, Responder, get, web};
use x402_actix::{
    facilitator_client::FacilitatorClient, middleware::X402Middleware, price::IntoPriceTag,
};
use x402_rs::{
    address_evm, address_sol,
    network::{Network, USDCDeployment},
};

#[get("/pay")]
async fn pay(
    req: actix_web::HttpRequest,
    x402: web::Data<X402Middleware<FacilitatorClient>>,
) -> Result<impl Responder, actix_web::Error> {
    let uri = req.uri();
    let paygate = x402.to_paygate(uri);
    let payload = paygate.extract_payment_payload(req.headers()).await?;
    let r = paygate.verify_payment(payload).await?;
    paygate.settle_payment(&r).await?;
    Ok("Hello, World!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let facilitator_url = std::env::var("FACILITATOR_URL")
        .unwrap_or_else(|_| "https://facilitator.x402.rs".to_string());
    let x402 = X402Middleware::try_from(facilitator_url)
        .unwrap()
        .with_base_url(url::Url::parse("https://localhost:3000/").unwrap())
        .with_mime_type("text/plain")
        .with_price_tag(
            USDCDeployment::by_network(Network::SolanaDevnet)
                .pay_to(address_sol!("EGBQqKn968sVv5cQh5Cr72pSTHfxsuzq7o7asqYB5uEV"))
                .amount(0.0025)
                .unwrap(),
        )
        .or_price_tag(
            USDCDeployment::by_network(Network::BaseSepolia)
                .pay_to(address_evm!("0xBAc675C310721717Cd4A37F6cbeA1F081b1C2a07"))
                .amount(0.0025)
                .unwrap(),
        );

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(x402.clone()))
            .service(pay)
    })
    .bind(("127.0.0.1", 3000))?
    .run()
    .await
}
