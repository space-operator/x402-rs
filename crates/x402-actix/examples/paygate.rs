use actix_web::{App, HttpServer, Responder, get, web};
use x402_actix::{
    facilitator_client::FacilitatorClient, middleware::X402Middleware, price::IntoPriceTag,
};
use x402_rs::{
    address_sol,
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
async fn main() {
    let facilitator_url = "https://www.x402.org/facilitator/".to_string();
    // "https://facilitator.x402.rs"
    let facilitator = FacilitatorClient::try_new(facilitator_url.parse().unwrap()).unwrap();
    let x402 = X402Middleware::new(facilitator)
        .await
        .unwrap()
        .with_base_url("https://localhost:3000/".parse().unwrap())
        .with_mime_type("text/plain")
        .with_price_tag(
            USDCDeployment::by_network(Network::SolanaDevnet)
                .pay_to(address_sol!("F9qRATtMLUdj11SEgZZV6QG5SK6zSTS2sEkxpRMTzE9Q"))
                .amount(0.0025)
                .unwrap(),
        );

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(x402.clone()))
            .service(pay)
    })
    .bind(("127.0.0.1", 3000))
    .unwrap()
    .run()
    .await
    .unwrap();
}
