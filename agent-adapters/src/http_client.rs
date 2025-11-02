use std::sync::Arc;

use hyper::client::HttpConnector;
use hyper::{Body, Client};
use hyper_rustls::HttpsConnector;
use rustls::{ClientConfig, OwnedTrustAnchor, RootCertStore};
use webpki_roots::TLS_SERVER_ROOTS;

use crate::traits::AdapterResult;

pub(crate) type HyperClient = Client<HttpsConnector<HttpConnector>, Body>;

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn build_https_client() -> AdapterResult<HyperClient> {
    let mut roots = RootCertStore::empty();
    roots.add_trust_anchors(TLS_SERVER_ROOTS.iter().map(|anchor| {
        OwnedTrustAnchor::from_subject_spki_name_constraints(
            anchor.subject,
            anchor.spki,
            anchor.name_constraints,
        )
    }));

    let config = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_no_client_auth();

    let mut http = HttpConnector::new();
    http.enforce_http(false);

    let connector = HttpsConnector::from((http, Arc::new(config)));

    Ok(Client::builder().build::<_, Body>(connector))
}
