use crate::provider::Provider;
use crate::providers::biquge_common::Biquge2Provider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(Biquge2Provider {
        name: "blqudu",
        base_url: "https://www.blqudu.cc",
    })
}
