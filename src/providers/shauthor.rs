use crate::provider::Provider;
use crate::providers::biquge_common::Biquge4Provider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(Biquge4Provider {
        name: "shauthor",
        base_url: "https://m.shauthor.com",
    })
}
