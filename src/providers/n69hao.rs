use crate::provider::Provider;
use crate::providers::biquge_common::Biquge3Provider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(Biquge3Provider {
        name: "n69hao",
        base_url: "https://www.69hao.com",
        search_url: "",
    })
}
