//! 内置分类知识库：常见网站域名与应用 → 分类。
//!
//! 解决"分类不准"的两个根源：
//! 1. 浏览器活动此前一律归 "browser"，不看站点内容——B 站和 GitHub 在统计里没有区别；
//! 2. 内置应用规则覆盖不足时直接落 "other"，只能靠用户手动加规则。
//!
//! 本表提供确定性的兜底知识；用户的 app_category_rules / website_semantic_rules
//! 始终优先（在 resolve_activity_classification 的应用顺序里保证）。
//! 表之外的未知实体再交给 AI 自动归类缓存（见 src-tauri 的 entity_classify_task）。

/// 域名 → (基础分类 key, 语义分类名)。
/// 匹配语义：域名等于条目、或以 ".条目" 结尾（自动覆盖 www./m. 等子域）；
/// 多条命中时取最长条目（更具体的子域条目优先，如 v.qq.com 优先于 qq.com）。
/// 基础分类为 "browser" 表示"保持浏览器归类不细分"，仅提供语义提示。
const DOMAIN_TABLE: &[(&str, &str, &str)] = &[
    // —— 开发:代码托管 / 技术问答 / 包注册表 / 文档 ——
    ("github.com", "development", "编码开发"),
    ("gitlab.com", "development", "编码开发"),
    ("gitee.com", "development", "编码开发"),
    ("bitbucket.org", "development", "编码开发"),
    ("stackoverflow.com", "development", "编码开发"),
    ("stackexchange.com", "development", "资料调研"),
    ("npmjs.com", "development", "编码开发"),
    ("pypi.org", "development", "编码开发"),
    ("crates.io", "development", "编码开发"),
    ("docs.rs", "development", "编码开发"),
    ("rust-lang.org", "development", "资料阅读"),
    ("developer.mozilla.org", "development", "资料阅读"),
    ("developer.apple.com", "development", "资料阅读"),
    ("developer.android.com", "development", "资料阅读"),
    ("learn.microsoft.com", "development", "资料阅读"),
    ("devdocs.io", "development", "资料阅读"),
    ("juejin.cn", "development", "资料阅读"),
    ("csdn.net", "development", "资料阅读"),
    ("segmentfault.com", "development", "资料阅读"),
    ("cnblogs.com", "development", "资料阅读"),
    ("oschina.net", "development", "资料阅读"),
    ("v2ex.com", "development", "资料阅读"),
    ("leetcode.com", "development", "编码开发"),
    ("leetcode.cn", "development", "编码开发"),
    ("kaggle.com", "development", "编码开发"),
    ("huggingface.co", "development", "编码开发"),
    ("vercel.com", "development", "编码开发"),
    ("netlify.com", "development", "编码开发"),
    ("cloudflare.com", "development", "编码开发"),
    ("console.aliyun.com", "development", "编码开发"),
    ("cloud.tencent.com", "development", "编码开发"),
    ("console.cloud.google.com", "development", "编码开发"),
    ("portal.azure.com", "development", "编码开发"),
    ("aws.amazon.com", "development", "编码开发"),
    ("jsfiddle.net", "development", "编码开发"),
    ("codepen.io", "development", "编码开发"),
    ("codesandbox.io", "development", "编码开发"),
    ("regex101.com", "development", "编码开发"),
    ("dockerhub.com", "development", "编码开发"),
    ("hub.docker.com", "development", "编码开发"),
    // —— AI 助手(知识工作,语义单列) ——
    ("chatgpt.com", "office", "AI 协作"),
    ("chat.openai.com", "office", "AI 协作"),
    ("claude.ai", "office", "AI 协作"),
    ("gemini.google.com", "office", "AI 协作"),
    ("poe.com", "office", "AI 协作"),
    ("perplexity.ai", "office", "AI 协作"),
    ("kimi.moonshot.cn", "office", "AI 协作"),
    ("kimi.com", "office", "AI 协作"),
    ("chat.deepseek.com", "office", "AI 协作"),
    ("tongyi.aliyun.com", "office", "AI 协作"),
    ("yiyan.baidu.com", "office", "AI 协作"),
    ("doubao.com", "office", "AI 协作"),
    ("chatglm.cn", "office", "AI 协作"),
    // —— 文档协作 / 笔记 / 任务管理(办公) ——
    ("docs.google.com", "office", "内容撰写"),
    ("drive.google.com", "office", "内容撰写"),
    ("notion.so", "office", "内容撰写"),
    ("notion.site", "office", "资料阅读"),
    ("yuque.com", "office", "内容撰写"),
    ("docs.qq.com", "office", "内容撰写"),
    ("shimo.im", "office", "内容撰写"),
    ("wolai.com", "office", "内容撰写"),
    ("atlassian.net", "office", "任务规划"),
    ("trello.com", "office", "任务规划"),
    ("asana.com", "office", "任务规划"),
    ("linear.app", "office", "任务规划"),
    ("teambition.com", "office", "任务规划"),
    ("tower.im", "office", "任务规划"),
    ("todoist.com", "office", "任务规划"),
    ("overleaf.com", "office", "内容撰写"),
    ("processon.com", "office", "内容撰写"),
    ("draw.io", "office", "内容撰写"),
    ("app.diagrams.net", "office", "内容撰写"),
    // —— 沟通 / 会议 / 邮箱 ——
    ("feishu.cn", "communication", "会议沟通"),
    ("larksuite.com", "communication", "会议沟通"),
    ("dingtalk.com", "communication", "会议沟通"),
    ("slack.com", "communication", "即时聊天"),
    ("discord.com", "communication", "即时聊天"),
    ("web.telegram.org", "communication", "即时聊天"),
    ("web.whatsapp.com", "communication", "即时聊天"),
    ("wx.qq.com", "communication", "即时聊天"),
    ("meet.google.com", "communication", "会议沟通"),
    ("zoom.us", "communication", "会议沟通"),
    ("teams.microsoft.com", "communication", "会议沟通"),
    ("meeting.tencent.com", "communication", "会议沟通"),
    ("mail.google.com", "communication", "内容撰写"),
    ("outlook.live.com", "communication", "内容撰写"),
    ("outlook.office.com", "communication", "内容撰写"),
    ("mail.qq.com", "communication", "内容撰写"),
    ("mail.163.com", "communication", "内容撰写"),
    // —— 设计 ——
    ("figma.com", "design", "设计创作"),
    ("canva.com", "design", "设计创作"),
    ("canva.cn", "design", "设计创作"),
    ("dribbble.com", "design", "设计创作"),
    ("behance.net", "design", "设计创作"),
    ("mastergo.com", "design", "设计创作"),
    ("js.design", "design", "设计创作"),
    ("unsplash.com", "design", "设计创作"),
    ("iconfont.cn", "design", "设计创作"),
    // —— 视频 / 直播(娱乐) ——
    ("youtube.com", "entertainment", "视频内容"),
    ("bilibili.com", "entertainment", "视频内容"),
    ("douyin.com", "entertainment", "视频内容"),
    ("tiktok.com", "entertainment", "视频内容"),
    ("iqiyi.com", "entertainment", "视频内容"),
    ("youku.com", "entertainment", "视频内容"),
    ("v.qq.com", "entertainment", "视频内容"),
    ("mgtv.com", "entertainment", "视频内容"),
    ("netflix.com", "entertainment", "视频内容"),
    ("twitch.tv", "entertainment", "视频内容"),
    ("douyu.com", "entertainment", "视频内容"),
    ("huya.com", "entertainment", "视频内容"),
    ("acfun.cn", "entertainment", "视频内容"),
    // —— 音乐 ——
    ("music.163.com", "entertainment", "音乐音频"),
    ("y.qq.com", "entertainment", "音乐音频"),
    ("spotify.com", "entertainment", "音乐音频"),
    ("music.apple.com", "entertainment", "音乐音频"),
    ("soundcloud.com", "entertainment", "音乐音频"),
    ("xiaoyuzhoufm.com", "entertainment", "音乐音频"),
    // —— 游戏 ——
    ("steampowered.com", "entertainment", "休息娱乐"),
    ("steamcommunity.com", "entertainment", "休息娱乐"),
    ("epicgames.com", "entertainment", "休息娱乐"),
    // —— 社交 / 购物(非工作) ——
    ("weibo.com", "entertainment", "休息娱乐"),
    ("xiaohongshu.com", "entertainment", "休息娱乐"),
    ("instagram.com", "entertainment", "休息娱乐"),
    ("facebook.com", "entertainment", "休息娱乐"),
    ("tieba.baidu.com", "entertainment", "休息娱乐"),
    ("hupu.com", "entertainment", "休息娱乐"),
    ("taobao.com", "entertainment", "休息娱乐"),
    ("tmall.com", "entertainment", "休息娱乐"),
    ("jd.com", "entertainment", "休息娱乐"),
    ("pinduoduo.com", "entertainment", "休息娱乐"),
    ("smzdm.com", "entertainment", "休息娱乐"),
    // —— 资讯 / 问答 / 检索(保持浏览器归类,只给语义) ——
    ("zhihu.com", "browser", "资料阅读"),
    ("twitter.com", "browser", "资料阅读"),
    ("x.com", "browser", "资料阅读"),
    ("reddit.com", "browser", "资料阅读"),
    ("news.ycombinator.com", "browser", "资料阅读"),
    ("medium.com", "browser", "资料阅读"),
    ("infoq.cn", "browser", "资料阅读"),
    ("36kr.com", "browser", "资料阅读"),
    ("sspai.com", "browser", "资料阅读"),
    ("ithome.com", "browser", "资料阅读"),
    ("wikipedia.org", "browser", "资料调研"),
    ("google.com", "browser", "资料调研"),
    ("bing.com", "browser", "资料调研"),
    ("baidu.com", "browser", "资料调研"),
    ("duckduckgo.com", "browser", "资料调研"),
    ("translate.google.com", "browser", "资料阅读"),
    ("deepl.com", "browser", "资料阅读"),
];

/// 应用名(小写包含匹配)→ 基础分类 key。
/// 是 categorize_app 硬编码链之外的增量覆盖,在其全部未命中、落 "other" 之前查询。
/// 模式须足够独特,避免子串误伤。
const APP_TABLE: &[(&str, &str)] = &[
    // 开发
    ("jupyter", "development"),
    ("rstudio", "development"),
    ("matlab", "development"),
    ("unity", "development"),
    ("unreal", "development"),
    ("godot", "development"),
    ("termius", "development"),
    ("xshell", "development"),
    ("finalshell", "development"),
    ("tabby", "development"),
    ("apifox", "development"),
    ("apipost", "development"),
    ("wireshark", "development"),
    ("virtualbox", "development"),
    ("vmware", "development"),
    ("parallels", "development"),
    ("utm", "development"),
    ("mongodb compass", "development"),
    ("redisinsight", "development"),
    ("another redis", "development"),
    // 办公 / 笔记 / 学术
    ("语雀", "office"),
    ("yuque", "office"),
    ("zotero", "office"),
    ("anki", "office"),
    ("calibre", "office"),
    ("goodnotes", "office"),
    ("notability", "office"),
    ("marginnote", "office"),
    ("有道", "office"),
    ("eudic", "office"),
    ("欧路词典", "office"),
    ("百度网盘", "office"),
    ("baidunetdisk", "office"),
    ("夸克", "office"),
    ("飞书文档", "office"),
    ("腾讯文档", "office"),
    ("mweb", "office"),
    ("思源笔记", "office"),
    ("siyuan", "office"),
    ("flomo", "office"),
    // 沟通 / 会议
    ("腾讯会议", "communication"),
    ("wemeet", "communication"),
    ("voov", "communication"),
    ("webex", "communication"),
    ("mattermost", "communication"),
    ("rocket.chat", "communication"),
    // 设计 / 创作
    ("blender", "design"),
    ("cinema 4d", "design"),
    ("davinci", "design"),
    ("premiere", "design"),
    ("final cut", "design"),
    ("after effects", "design"),
    ("audacity", "design"),
    ("剪映", "design"),
    ("capcut", "design"),
    ("obs", "design"),
    ("obs studio", "design"),
    ("procreate", "design"),
    ("即时设计", "design"),
    ("mastergo", "design"),
    // 娱乐
    ("腾讯视频", "entertainment"),
    ("优酷", "entertainment"),
    ("芒果tv", "entertainment"),
    ("抖音", "entertainment"),
    ("快手", "entertainment"),
    ("iina", "entertainment"),
    ("vlc", "entertainment"),
    ("potplayer", "entertainment"),
    ("mpv", "entertainment"),
    ("微信读书", "entertainment"),
    ("weread", "entertainment"),
    ("kindle", "entertainment"),
    ("crossover", "entertainment"),
];

/// 查询域名知识库。入参可为完整 URL 或裸域名;返回 (基础分类, 语义分类)。
/// 基础分类为 "browser" 的条目表示不细分基础分类、仅提供语义。
pub fn builtin_domain_category(url_or_domain: &str) -> Option<(&'static str, &'static str)> {
    let domain = crate::config::PrivacyConfig::extract_domain(url_or_domain);
    if domain.is_empty() {
        return None;
    }
    // 剥离端口(extract_domain 保留端口,如 localhost:5173)
    let domain = domain.split(':').next().unwrap_or("");

    let mut best: Option<(usize, &'static str, &'static str)> = None;
    for (suffix, base, semantic) in DOMAIN_TABLE {
        let hit = domain == *suffix || domain.ends_with(&format!(".{suffix}"));
        if hit && best.is_none_or(|(len, _, _)| suffix.len() > len) {
            best = Some((suffix.len(), base, semantic));
        }
    }
    best.map(|(_, base, semantic)| (base, semantic))
}

/// 词边界包含匹配:命中片段的前后紧邻字符不能是 ASCII 字母数字,
/// 防止短模式误伤("obs" 不应命中 "obsidian","unity" 不应命中 "community")。
/// 中文等非 ASCII 邻字符视为边界,因此中文模式不受影响。
fn contains_word(haystack: &str, needle: &str) -> bool {
    let mut search_from = 0;
    while let Some(pos) = haystack[search_from..].find(needle) {
        let start = search_from + pos;
        let end = start + needle.len();
        let before_ok = haystack[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        let after_ok = haystack[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        search_from = end.max(search_from + 1);
    }
    false
}

/// 查询应用知识库(入参需为小写应用名)。
pub fn builtin_app_category(app_lower: &str) -> Option<&'static str> {
    APP_TABLE
        .iter()
        .find(|(pattern, _)| contains_word(app_lower, pattern))
        .map(|(_, category)| *category)
}

#[cfg(test)]
mod tests {
    use super::{builtin_app_category, builtin_domain_category};

    #[test]
    fn 域名知识库应支持子域与最长后缀优先() {
        assert_eq!(
            builtin_domain_category("https://github.com/org/repo/pull/1"),
            Some(("development", "编码开发"))
        );
        assert_eq!(
            builtin_domain_category("https://www.bilibili.com/video/BV1"),
            Some(("entertainment", "视频内容"))
        );
        // 同一主域下的不同子域各自命中自己的条目
        assert_eq!(
            builtin_domain_category("https://v.qq.com/x/cover"),
            Some(("entertainment", "视频内容"))
        );
        assert_eq!(
            builtin_domain_category("https://mail.qq.com/inbox"),
            Some(("communication", "内容撰写"))
        );
        // 最长后缀优先:tieba.baidu.com(贴吧)胜过 baidu.com(搜索)
        assert_eq!(
            builtin_domain_category("https://tieba.baidu.com/p/123"),
            Some(("entertainment", "休息娱乐"))
        );
        // 资讯类保持 browser 基础分类,只给语义
        assert_eq!(
            builtin_domain_category("zhihu.com"),
            Some(("browser", "资料阅读"))
        );
        // 未知域名与带端口的本地地址不给出结论
        assert_eq!(builtin_domain_category("https://unknown-site.example"), None);
        assert_eq!(builtin_domain_category("http://localhost:5173/app"), None);
    }

    #[test]
    fn 应用知识库应覆盖硬编码链之外的常见应用() {
        assert_eq!(builtin_app_category("腾讯会议"), Some("communication"));
        assert_eq!(builtin_app_category("jupyter notebook"), Some("development"));
        assert_eq!(builtin_app_category("剪映专业版"), Some("design"));
        assert_eq!(builtin_app_category("微信读书"), Some("entertainment"));
        assert_eq!(builtin_app_category("some unknown app"), None);
    }

    #[test]
    fn 应用匹配应按词边界防止短模式误伤() {
        assert_eq!(builtin_app_category("obs"), Some("design"));
        assert_eq!(builtin_app_category("obs studio 30"), Some("design"));
        // obsidian 不应被 "obs" 命中(它由 categorize_app 硬编码链归入 office,
        // 即便走到知识库也必须返回 None)
        assert_eq!(builtin_app_category("obsidian"), None);
        // community 不应被 "unity" 命中
        assert_eq!(builtin_app_category("community"), None);
        assert_eq!(builtin_app_category("unity hub"), Some("development"));
    }
}
