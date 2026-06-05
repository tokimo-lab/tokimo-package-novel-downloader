use crate::provider::Provider;

// Common shared implementations
pub mod biquge_common;
pub mod mangg_search;

// --- Provider modules ---
// Biquge group
pub mod biquge5;
pub mod biquguo;
pub mod blqudu;
pub mod bxwx9;
pub mod ciluke;
pub mod fsshu;
pub mod ktshu;
pub mod lewenn;
pub mod mangg_net;
pub mod mjyhb;
pub mod n23ddw;
pub mod n37yue;
pub mod n69hao;
pub mod shauthor;

// Search providers A
pub mod alicesw;
pub mod b520;
pub mod biquge345;
pub mod bixiange;
pub mod ciyuanji;
pub mod czbooks;
pub mod dxmwx;
pub mod esjzone;
pub mod haiwaishubao;
pub mod hetushu;
pub mod i25zw;

// Search providers B
pub mod ixdzs8;
pub mod jpxs123;
pub mod laoyaoxs;
pub mod n101kanshu;
pub mod n23qb;
pub mod n37yq;
pub mod n71ge;
pub mod piaotia;
pub mod qbtr;
pub mod quanben5;
pub mod shuhaige;

// Search providers C
pub mod kadokado;
pub mod linovel;
pub mod n8novel;
pub mod tongrenquan;
pub mod tongrenshe;
pub mod trxs;
pub mod ttkan;
pub mod uaa;
pub mod xiguashuwu;
pub mod xshbook;
pub mod yodu;

// Complex providers
pub mod ciweimao;
pub mod fanqienovel;
pub mod linovelib;
pub mod qidian;
pub mod sfacg;
pub mod wenku8;

// No-search providers A
pub mod akatsuki_novels;
pub mod alphapolis;
pub mod dushu;
pub mod faloo;
pub mod guidaye;
pub mod hongxiuzhao;
pub mod kunnu;
pub mod lnovel;
pub mod lvsewx;
pub mod mangg_com;
pub mod novelpia;
pub mod pilibook;
pub mod ruochu;
pub mod shaoniandream;
pub mod shencou;

// No-search providers B
pub mod n17k;
pub mod n69shuba;
pub mod qqbook;
pub mod shu111;
pub mod syosetu;
pub mod syosetu18;
pub mod syosetu_org;
pub mod tianyabooks;
pub mod twkan;
pub mod westnovel;
pub mod westnovel_sub;
pub mod wxsck;
pub mod yamibo;
pub mod yibige;
pub mod zhenhunxiaoshuo;

/// Get all available providers
pub fn get_all_providers() -> Vec<Box<dyn Provider>> {
    vec![
        // Biquge group
        biquge5::provider(),
        biquguo::provider(),
        bxwx9::provider(),
        ciluke::provider(),
        fsshu::provider(),
        ktshu::provider(),
        n37yue::provider(),
        mangg_net::provider(),
        blqudu::provider(),
        lewenn::provider(),
        n23ddw::provider(),
        n69hao::provider(),
        mjyhb::provider(),
        shauthor::provider(),
        // Search providers A
        alicesw::provider(),
        b520::provider(),
        biquge345::provider(),
        bixiange::provider(),
        ciyuanji::provider(),
        czbooks::provider(),
        dxmwx::provider(),
        esjzone::provider(),
        haiwaishubao::provider(),
        hetushu::provider(),
        i25zw::provider(),
        // Search providers B
        ixdzs8::provider(),
        jpxs123::provider(),
        laoyaoxs::provider(),
        n101kanshu::provider(),
        n23qb::provider(),
        n37yq::provider(),
        n71ge::provider(),
        piaotia::provider(),
        qbtr::provider(),
        quanben5::provider(),
        shuhaige::provider(),
        // Search providers C
        tongrenquan::provider(),
        tongrenshe::provider(),
        trxs::provider(),
        ttkan::provider(),
        uaa::provider(),
        xiguashuwu::provider(),
        yodu::provider(),
        xshbook::provider(),
        kadokado::provider(),
        n8novel::provider(),
        linovel::provider(),
        // Complex providers
        qidian::provider(),
        linovelib::provider(),
        wenku8::provider(),
        sfacg::provider(),
        ciweimao::provider(),
        fanqienovel::provider(),
        // No-search providers A
        akatsuki_novels::provider(),
        alphapolis::provider(),
        dushu::provider(),
        faloo::provider(),
        guidaye::provider(),
        hongxiuzhao::provider(),
        kunnu::provider(),
        lnovel::provider(),
        lvsewx::provider(),
        mangg_com::provider(),
        novelpia::provider(),
        pilibook::provider(),
        ruochu::provider(),
        shaoniandream::provider(),
        shencou::provider(),
        // No-search providers B
        shu111::provider(),
        syosetu::provider(),
        syosetu18::provider(),
        syosetu_org::provider(),
        tianyabooks::provider(),
        twkan::provider(),
        westnovel::provider(),
        westnovel_sub::provider(),
        wxsck::provider(),
        yamibo::provider(),
        yibige::provider(),
        zhenhunxiaoshuo::provider(),
        n17k::provider(),
        n69shuba::provider(),
        qqbook::provider(),
    ]
}
