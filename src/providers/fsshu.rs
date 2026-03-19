use crate::provider::Provider;
use crate::providers::biquge_common::Biquge1Provider;

pub fn provider() -> Box<dyn Provider> {
    Box::new(Biquge1Provider {
        name: "fsshu",
        base_url: "https://www.fsshu.com",
        search_url: "https://www.fsshu.com/search.php",
        use_paginated_info: true,
        use_paginated_chapter: true,
    })
}
