# 设置页设计探索

> 基准:`/home/user/out/overview-redesign-mockup.html` 五原则。
> 现状源码:`src/routes/settings/Settings.svelte`(436 行)+ `components/` 下 SettingsGeneral(472 行)/ SettingsPrivacy(436)/ SettingsStorage(966)/ SettingsSystem(837);Settings.svelte L10–13 还引用了 SettingsAppearance / SettingsAI / SettingsAvatar / SettingsNodeGateway(未随包提供,按引用与 tab 定义推断);样式在 `src/app.css` L2221–2349。
> Mockup:`assets/settings-mockup.html`(代表性屏:左侧分组导航 + 右侧「通用 · 时间与记录」重构示例 + 底部未保存更改条)。

## 一、现状盘点

### 结构与区块

- **页头**:标题+副标题+右上唯一「保存」按钮(Settings.svelte L306–339);sticky+毛玻璃(app.css L2226–2237)。
- **macOS 权限卡**:`settingsRuntimePlatform === 'macos'` 时 SettingsSystem 整卡永远置顶(L355–359),组件本身 837 行(权限检测/引导弹窗,SettingsSystem.svelte L423 权限设置标题)。
- **左侧 tab 栏**:7 个入口 general / appearance / ai / avatar(Beta)/ privacy / storage / node(Beta)(L36–44),12.5rem 宽 rail(app.css L2255–2260),Beta 徽章内联 hardcode(L387–391),激活态 indigo 渐变(app.css L2301–2308)。
- **右侧 stage**:按 activeTab 条件渲染子组件(L397–431)。
- **tab 内部再分层**(以随包组件为证):
  - General:工作时段(L215–287)+ 折叠「高级工时」(L290–339)+ 折叠「工作目标」(L343–370)+ 折叠「AI 工作记忆」(L373–381)+ 日报自动生成时间(L382–410)+ 系统行为 4 开关(L413–469)。
  - Storage:截图卡(L235)/ 宽度模式卡(L385)/ 日报导出卡(L455–499)/ 折叠「远程存储」S3+WebDAV 表单(L501–804)/ 数据目录卡(L806–882,含迁移、旧目录清理)/ 存储用量+清理块(L884–964)。
  - Privacy:应用规则+快捷选择(L126–230+)/ 排除域名 / 排除关键词。
  - 合计可数分区 **15+**(7 tab × 内部 4–6 节),其中折叠段展开态存 localStorage(CollapsibleSection `storageKey`,SettingsGeneral L290/343/373)。

### 保存与状态模型

- `dirty` 为组件内布尔(L20),各子组件 `on:change` 置位(L399–429);Appearance 走 `autosaved` 标记绕过 dirty(L254–256、L401)。
- 保存:唯一按钮调 `save_config`(L229–252),成功 toast+按钮 3 秒变「已保存」;`cache.subscribe` 在 saving/dirty 时跳过覆盖以防丢改动(L286–293)。
- **即时生效的例外**:SettingsGeneral 的开机自启/Dock 图标/启动模式直接 invoke 并自行 `save_config`(L154–208);存储 tab 的测试连接、目录迁移即点即执行(SettingsStorage L792–799、L824–834)。
- 危险操作:「清理历史数据」红面板与普通「清理缓存」同列于 Storage 底部(SettingsStorage L939–963),无二次确认 UI;数据目录迁移/清理旧目录紧邻其上(L806–880)。
- 加载:整页 spinner(L341–343);loadConfig 内 ~130 行字段默认值补丁(L69–198)。

## 二、问题清单

1. **导航负担:15+ 分区两级藏一级(可用性)** — rail 只有 7 个入口(L36–44),真正的设置项藏在 tab 内的卡与 localStorage 记忆的折叠段里(SettingsGeneral L290/343/373);找「空闲检测阈值」需 通用→高级工时→展开 三跳,且无搜索、无从其他页深链(activeTab 是内存变量 L34,刷新即回 general)。
2. **dirty 不可见,保存可发现性差(可用性)** — 有未保存更改时,「保存」按钮外观零变化(L322–338 仅 saving/success 两态);切 tab、离开页面均无拦截或提示,改动静默丢失;`cache.subscribe` 的防覆盖补丁(L286–293)恰说明已经踩过丢改动的坑。
3. **三种保存语义混用,心智无法建立(可用性)** — 同一页面里:①显式保存(大多数字段)②即时生效并自写盘(自启/Dock,SettingsGeneral L154–208)③autosaved 事件(Appearance,L254–256)。界面上三者毫无区分标注。
4. **危险操作未隔离(可用性)** — 「清理历史数据」与日常「清理缓存」同一个列表块(SettingsStorage L939–963),仅按钮变红;上方紧邻目录迁移;无输入确认/影响预估(会删多少条、多少 MB)。
5. **权限卡永久占用首屏(违反原则③)** — macOS 上 SettingsSystem 全量置顶(L355–359),权限全部正常时依旧占一屏头部,把高频设置往下推。
6. **卡内是「表单堆」不是「故事」(违反原则②)** — 工作时段是纯 time input 行列表(SettingsGeneral L242–276),没有 24h 可视化预览;标准工时/目标等数字无参照(如「本周平均 6h48m」),违反原则①(数字带参照系)。
7. **视觉 token 漂移(违反原则⑤)** — rail/激活态用 indigo(app.css L2301–2308)而非 primary #3b82f6;卡圆角 1.7rem≈27px(app.css L2233、2267、2327)偏离 22px;Beta 徽章 amber 内联样式(Settings.svelte L387–391)未 token 化。
8. **状态反馈分散** — 保存成功同时有 toast(L240)和按钮态(L237–246)双通道;错误 banner 只在顶部(L345–352),长页底部操作出错需回滚顶部查看。

## 三、改版方案

### 信息层级(代表性屏见 mockup)

```
页头:设置 · 一句副标题(右侧不再放保存按钮)
├ 左导航(240px,分组 + 搜索)
│   搜索框(索引到「项」级,含折叠段内条目)
│   记录:通用(时间与记录)· 隐私
│   智能:AI 模型 · 数字形象 Beta
│   数据:存储与截图 · 导出与备份
│   接入:节点网关 Beta · 消息机器人
│   系统:权限状态(状态点:1 项待授权)· 外观
└ 右内容列:分区标题 → 故事卡(每卡一个决策)→ 行
    卡标题右侧标注保存语义 chip:「即时生效」/「需保存」
底部 sticky 未保存更改条:「N 项未保存:工作时段、轻量模式」 放弃 | 保存更改
```

### 关键交互

1. **分组导航+搜索**:7 个平级 tab 重组为 5 组 9–10 项,组名给扫读锚点;搜索命中直接跳转并高亮目标行、自动展开所在折叠段(解决三跳问题);activeTab 写入 URL hash,支持「助手页→去配置模型」深链。
2. **统一保存模型**:字段级标注两种语义——「即时生效」chip(开关类系统项)与默认「需保存」;任何需保存字段变更后,底部浮出 dirty bar(列出改了哪些项),保存/放弃都在此;切分区不清空,离开页面弹拦截。顶部不再放保存按钮(移除 save-dock,L322–338)。
3. **权限收编**:macOS 权限从置顶整卡改为导航「系统」项上的状态点+进入后的状态卡;仅当有待授权项时,页头下方出现一条 insight 式提醒条(复用概览洞察条样式)。
4. **危险区隔离**:存储分区末尾独立「危险区」卡(红描边、单独标题),清理历史需二段确认+影响预估(「将删除 32 天前的 12,480 条记录 · 约 860MB」);与目录迁移分卡。
5. **故事卡示例(工作时段)**:卡首行=结论(「每日工作时段合计 7h30m,作为加班与占比的基准」);中部 24h 预览条(时段着 primary 色,复用概览细进度条 token);下方才是时段编辑行;高级项(空闲阈值/标准工时)保持折叠但可被搜索。

## 四、落地清单(按工作量升序)

| # | 改动 | 工作量 | 改动面 |
|---|---|---|---|
| 1 | 保存语义 chip(即时生效/需保存)标注 | 小 | 各 Settings*.svelte 卡标题行 + app.css 新增 chip 类 |
| 2 | 危险区独立卡+二段确认+影响预估 | 小中 | SettingsStorage.svelte L939–963;后端补「预估删除量」查询 |
| 3 | activeTab 写入 URL(hash/query),支持深链 | 小中 | Settings.svelte L34、L363–394;调用方(如 Ask 页)加链接 |
| 4 | dirty bar(底部未保存更改条+离开拦截) | 中 | Settings.svelte L20、L229–256、L322–338 移除顶部按钮;新增 DirtyBar 组件;dirty 从布尔升级为「变更字段集合」 |
| 5 | 导航分组重构(5 组)+ Beta 徽章 token 化 | 中 | Settings.svelte L36–44、L362–395;app.css L2262–2320 |
| 6 | macOS 权限卡收编为状态点+提醒条 | 中 | Settings.svelte L355–359;SettingsSystem.svelte 拆状态摘要 |
| 7 | 设置项搜索(项级索引+跳转高亮+自动展开) | 中大 | 新增索引表(id→分区/折叠段/i18n key);CollapsibleSection 支持受控展开 |
| 8 | 工作时段故事卡(24h 预览条+结论行) | 中大 | SettingsGeneral.svelte L215–287 重构;复用概览 compo 条样式 |
| 9 | 视觉 token 对齐(22px/primary/slate) | 中大 | app.css L2221–2349、L2787–2820(dark)全段梳理 |
| 10 | config 默认值补丁下沉后端/独立 schema 层 | 大(工程) | Settings.svelte L69–198 迁出;Rust 侧 serde default |
