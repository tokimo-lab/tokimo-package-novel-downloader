use crate::provider::Provider;
use crate::providers::biquge_common::Biquge1Provider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(Biquge1Provider {
        name: "mangg_net",
        base_url: "https://www.mangg.net",
        search_url: "https://www.mangg.net/search.php",
        use_paginated_info: true,
        use_paginated_chapter: true,
    })
}
