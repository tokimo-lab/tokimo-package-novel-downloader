# Novel Downloader (Rust)

命令行小说下载器 - 搜索并下载网络小说，支持 83 个站点。

Rust 重写版本，基于 [saudadez21/novel-downloader](https://github.com/saudadez21/novel-downloader)。

## 功能

- 🔍 跨多站点搜索小说（42 个站点支持搜索）
- 📥 下载小说并保存为 TXT 文件
- 📊 下载进度条显示
- 🌐 支持 83 个小说站点
- 🔤 自动编码检测（UTF-8、GBK、Big5、EUC-JP 等）

## 安装

```bash
# 克隆项目
git clone <repo-url>
cd novel-downloader-rs

# 编译
cargo build --release

# 可执行文件在 target/release/novel-downloader
```

## 使用方法

### 交互模式（默认）

```bash
./novel-downloader
# 输入小说名称，选择搜索结果，自动下载
```

### 搜索模式

```bash
./novel-downloader search 斗罗大陆
```

### 直接下载

```bash
./novel-downloader download <provider> <book_id> [output.txt]
```

### 查看支持站点

```bash
./novel-downloader list
```

## 支持站点（83 个）

### 支持搜索（42 个）

| 站点 | URL |
|------|-----|
| biquge5 | biquge5.com |
| biquguo | biquguo.com |
| bxwx9 | bxwx9.org |
| ciluke | ciluke.com |
| fsshu | fsshu.com |
| ktshu | ktshu.cc |
| n37yue | 37yue.com |
| mangg_net | mangg.net |
| n23ddw | 23ddw.net |
| alicesw | alicesw.com |
| b520 | b520.cc |
| biquge345 | biquge345.com |
| bixiange | bixiange.me |
| ciyuanji | ciyuanji.com |
| czbooks | czbooks.net |
| dxmwx | dxmwx.org |
| esjzone | esjzone.cc |
| haiwaishubao | haiwaishubao.com |
| hetushu | hetushu.com |
| i25zw | i25zw.com |
| ixdzs8 | ixdzs8.com |
| jpxs123 | jpxs123.com |
| laoyaoxs | laoyaoxs.org |
| n101kanshu | 101kanshu.com |
| n23qb | 23qb.com |
| n37yq | 37yq.com |
| n71ge | 71ge.com |
| piaotia | piaotia.com |
| qbtr | qbtr.cc |
| quanben5 | quanben5.com |
| shuhaige | shuhaige.net |
| tongrenquan | tongrenquan.org |
| tongrenshe | tongrenshe.cc |
| trxs | trxs.cc |
| ttkan | ttkan.co |
| uaa | uaa.com |
| xiguashuwu | xiguashuwu.com |
| yodu | yodu.org |
| kadokado | kadokado.com.tw |
| n8novel | 8novel.com |
| linovel | linovel.net |
| qidian | qidian.com |

### 仅下载（41 个）

| 站点 | URL |
|------|-----|
| blqudu | blqudu.cc |
| lewenn | lewenn.net |
| n69hao | 69hao.com |
| mjyhb | mjyhb.com |
| shauthor | shauthor.com |
| xshbook | xshbook.com |
| linovelib | linovelib.com |
| wenku8 | wenku8.net |
| sfacg | sfacg.com |
| ciweimao | ciweimao.com |
| fanqienovel | fanqienovel.com |
| akatsuki_novels | akatsuki-novels.com |
| alphapolis | alphapolis.co.jp |
| dushu | dushu.com |
| faloo | faloo.com |
| guidaye | guidaye.com |
| hongxiuzhao | hongxiuzhao.net |
| kunnu | kunnu.com |
| lnovel | lnovel.org |
| lvsewx | lvsewx.cc |
| mangg_com | mangg.com |
| novelpia | novelpia.com |
| pilibook | pilibook.net |
| ruochu | ruochu.com |
| shaoniandream | shaoniandream.com |
| shencou | shencou.com |
| shu111 | shu111.com |
| syosetu | syosetu.com |
| syosetu18 | novel18.syosetu.com |
| syosetu_org | syosetu.org |
| tianyabooks | tianyabooks.com |
| twkan | twkan.com |
| westnovel | westnovel.com |
| westnovel_sub | westnovel.com |
| wxsck | wxsck.com |
| yamibo | yamibo.com |
| yibige | yibige.com |
| zhenhunxiaoshuo | zhenhunxiaoshuo.com |
| n17k | 17k.com |
| n69shuba | 69shuba.com |
| qqbook | qqbook.com |

## 示例

搜索并下载《斗罗大陆》：

```bash
$ ./novel-downloader search 斗罗大陆
Searching '斗罗大陆' across 42 providers...

  ✓ hetushu (5 results)
  ✓ dxmwx (5 results)
  ✓ i25zw (5 results)
  ...

Found 67 results:

[ 1] 斗罗大陆 - 唐家三少 [dxmwx]
[ 2] 斗罗大陆 - 唐家三少 [hetushu]
...

Select a novel to download: 1

Downloading...
⠋ [00:05:32] [################>-----------------------] 226/686 (12m)

Saved to: 斗罗大陆.txt (686 chapters downloaded, 0 failed)
```

## 技术栈

- **语言**: Rust
- **HTTP**: reqwest (支持 cookies、gzip、brotli)
- **HTML 解析**: scraper (CSS 选择器)
- **异步运行时**: tokio
- **CLI**: dialoguer + indicatif
- **编码**: encoding_rs (GBK/Big5/EUC-JP 等)

## 注意事项

- 部分站点可能需要科学上网访问
- 下载速度受站点限制，每章间隔 100ms 以避免被封
- VIP/付费章节无法下载
- 部分站点可能已失效或变更
