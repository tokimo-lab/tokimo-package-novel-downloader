use crate::provider::Provider;
use crate::providers::biquge_common::Biquge3Provider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(Biquge3Provider {
        name: "n23ddw",
        base_url: "https://www.23ddw.net",
        search_url: "https://www.23ddw.net/searchss/",
    })
}
