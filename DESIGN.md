---
name: Kindle 信息屏
description: 墨水屏信息牌的配置控制台 —— 翻牌站牌视觉世界
colors:
  ground: "#0f0f0f"
  panel: "#121212"
  panel-warm: "#16130c"
  flap-lit: "#1a1a1a"
  flap-shadow: "#050505"
  input-well: "#060606"
  letter: "#f2f2ef"
  dim: "#969696"
  steel-frame: "#2a2d31"
  steel-edge: "#586060"
  hairline: "#26292d"
  amber: "#ffb000"
  failure-surface: "#b00d20"
  failure-text: "#ff6b6b"
typography:
  body:
    fontFamily: "-apple-system, 'PingFang SC', 'Microsoft YaHei', sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
  data:
    fontFamily: "'SF Mono', Menlo, Consolas, monospace"
    fontSize: "14px"
    fontWeight: 400
  display:
    fontFamily: "-apple-system, 'PingFang SC', sans-serif"
    fontSize: "14px"
    fontWeight: 900
    letterSpacing: "5px"
rounded:
  sm: "3px"
  md: "4px"
  lg: "6px"
components:
  flap:
    backgroundColor: "linear-gradient({colors.flap-lit} 0 47%, {colors.flap-shadow} 53% 100%)"
    textColor: "{colors.letter}"
    rounded: "{rounded.sm}"
    padding: "2px 9px"
  flap-status-amber:
    textColor: "{colors.amber}"
  flap-status-failure:
    textColor: "{colors.failure-text}"
  board-row-selected:
    backgroundColor: "#211d10"
    textColor: "{colors.amber}"
  button-primary:
    backgroundColor: "{colors.amber}"
    textColor: "{colors.ground}"
    rounded: "{rounded.sm}"
    padding: "9px 26px"
  button-secondary:
    backgroundColor: "{colors.input-well}"
    textColor: "{colors.letter}"
    rounded: "{rounded.sm}"
    padding: "9px 26px"
  toggle-on:
    backgroundColor: "{colors.amber}"
    textColor: "{colors.ground}"
  input:
    backgroundColor: "{colors.input-well}"
    textColor: "{colors.letter}"
    rounded: "{rounded.sm}"
    padding: "5px 9px"
---

# Overview

信息屏控制台是「车站翻牌出发信息牌」:每个配置模板是一班车,模块清单是途经站,渲染状态以翻牌格呈现。世界只向页面出借四样东西——漆白字、钢框灰阶、琥珀状态色、翻牌格中缝;布局与控件一律是平台标准件。整页**只有一个字号(14px)**,等级全靠字重(700/900)、反白、界行与色块承担。

# Colors

- ** jurisdictions(职务制)**:琥珀 `#ffb000` 只属于状态与可动之处;`#b00d20` 只做失败状态的表面(toast),失败文字用其可读浅调 `#ff6b6b`;灰阶只做结构。
- 钢框:`#2a2d31` 主框 + `#586060` 亮边(40% 透明度内描),界行 `#26292d`——灰要带绿灰的"钢"味,不用中性灰。
- 场地:`#0f0f0f` 页面场,`#121212` 面板,`#16130c` 琥珀暖光的工单区(车站被琥珀灯照亮的角落)。
- 翻牌面:上半 `#1a1a1a` 受光、下半 `#050505` 背光,中缝是全页唯一纹理。

# Typography

单字号纪律:全页只有 14px。层级 = 字重(400/700/900)× 字距(1–5px)× 反白 × 等宽/黑体分工。数据(时间、地址、尺寸)一律等宽;拉丁牌面词(PLATFORM、STATUS)等宽 + 2px 字距;中文标题黑体 900 + 宽字距。

# Layout

桌面首屏 = 钢框牌头(题名/平台号/上次上屏灯)→ 4 列固定栅格翻牌板(SERVICE/MODULES/ORIENTATION/STATUS)→ 工单 + Departures 双栏。分区用整块色域(色块分区),不用线框描边。≤700px:模块列隐藏、牌头收起平台牌、`#lastRender` 截断、标题字距收窄保持单行。

# Elevation & Depth

无阴影、无发光。层次由场地色差(ground < panel)与硬投影(`4px 4px 0` 仅用于决策页骨架)承担。状态灯允许 6px 同色光晕,仅限牌头灯。

# Shapes

圆角 3/4/6px,克制统一。翻牌格的视觉签名是上下两截的渐变中缝,不是圆角。

# Components

- **flap(翻牌格)**:状态专用,不做按钮。`.amber` 琥珀字、`.red` 失败字、`.dim` 未激活。状态变化时 `rotateX` 翻转一次(320ms)。
- **board row**:整行可点选模板;选中行 `inset 3px` 琥珀条 + 琥珀服务名。
- **toggle**:反白条语义——开 = 琥珀底黑字 900;关 = 描边灰字。
- **input**:黑井 `#060606` 等宽;聚焦琥珀描边;全局 caret/选区/滚动条同琥珀主题。
- **departures row**:渲染史账目行,落款不消失,失败行红字保留。

# Do's and Don'ts

- ✅ 状态翻牌、账目落款、琥珀只给可动与状态、单一字号。
- ❌ 不给翻牌格以外的元素做中缝纹理;不用琥珀以外的强调色;不用字号做层级;不弹 toast 之外浮层(toast 本身琥珀=成功、红=失败);不把 raw `#b00d20` 用于文字。
