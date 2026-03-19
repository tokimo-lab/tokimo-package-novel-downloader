use crate::provider::Provider;
use crate::providers::biquge_common::Biquge1Provider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(Biquge1Provider {
        name: "ktshu",
        base_url: "https://www.ktshu.cc",
        search_url: "https://www.ktshu.cc/search.php",
        use_paginated_info: true,
        use_paginated_chapter: true,
    })
}
