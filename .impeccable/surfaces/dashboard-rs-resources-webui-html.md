---
version: 1
slug: "dashboard-rs-resources-webui-html"
primary_target: "dashboard-rs/resources/webui.html"
related_targets: []
---

# Surface brief — Web 配置控制台 (dashboard-rs/resources/webui.html)

**Scope/Mode**: Operate。信息屏主人的日常操作台:换版面、改参数、管模块、看渲染状态。
**Audience/Job**: 仓库主人;路过改一条待办、换一套版面、看上屏状态;10 秒内完成常见操作。
**Constraints**: 零依赖单文件(HTML+CSS+vanilla JS);API 契约不变;双后端共用;移动端可扫。

## Direction contract
THESIS: 控制台就是出发信息牌——每个模板是一班"车次",模块是途经站,渲染状态在牌上翻落;拒绝"通用设置后台"的白卡片排布。
OWN-WORLD: 磨砂黑翻牌面(#0b0b0d)漆白字(#f2f2f2),钢框灰阶(#2a2d31/#586060)做框与界行,琥珀(#ffb000)只做状态与主行动,取消红(#b00d20)只做失败;等宽数据 + 白漆字;翻牌格中缝是唯一纹理。
STORY: 打开即读"现在哪班在跑、上次几点上屏、下轮何时";改完保存,状态格逐格翻成 RENDERING→ON TIME,渲染史多一行落款;一切状态可回溯,不弹走。
FIRST VIEWPORT (1440×900): 顶部钢框牌头(题名+平台号+状态灯);主牌为 4 列固定栅格的翻牌板(SERVICE/MODULES/ORIENTATION/STATUS),当前模板行高亮琥珀;牌下双栏——左"工单"(城市/间隔/端口/日程/热榜/壁纸),右"DEPARTURES 渲染史"账目行;主行动「刷新上屏」琥珀钮在工单尾。
FORM: 挑战者全卡 3 张(玻璃片架 competitive、票夹 declined、园区图 declined),主方向带 3 条具名提升(单一字号/色块分区/官方套色克制)。seed: e0be6f8d0110fc36
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.

## Unresolved
- 移动端 390px 下翻牌板降级为双列(时间/目的地优先)。
