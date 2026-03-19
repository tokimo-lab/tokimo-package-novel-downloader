use crate::provider::Provider;
use crate::providers::biquge_common::Biquge1Provider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(Biquge1Provider {
        name: "bxwx9",
        base_url: "https://www.bxwx9.org",
        search_url: "https://www.bxwx9.org/search.php",
        use_paginated_info: true,
        use_paginated_chapter: true,
    })
}
