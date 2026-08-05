## BongoCat 资源来源

当前桌宠默认形象直接复用以下开源资源：

- 上游仓库：`MMmmmoko/Bongo-Cat-Mver`
- 仓库地址：<https://github.com/MMmmmoko/Bongo-Cat-Mver>
- 许可证：MIT

本目录当前使用的主资源链：

- `mouse-bg.png`
  来源：`BongoCatMver/img/standard/mousebg.png`
- `mouse.png` / `mouse_left.png` / `mouse_right.png` / `mouse_side.png`
  来源：`BongoCatMver/img/standard/`
- `standard-up.png`
  来源：`BongoCatMver/img/standard/up.png`
- `standard-hand-0.png` 到 `standard-hand-14.png`
  来源：`BongoCatMver/img/standard/hand/`
- `standard-keyboard-0.png` 到 `standard-keyboard-14.png`
  来源：`BongoCatMver/img/standard/keyboard/`
- `arm.png`
  来源：`BongoCatMver/img/standard/arm.png`
  说明：上游标准模式并不是直接把这张图整张叠上去，而是配合运行时几何手臂一起做填充与描边。

接入原则：

- 优先复用上游原图，不再在项目内手绘一版近似轮廓。
- 标准模式的主场景、键盘高亮和右手帧全部按上游 `config.json` 与 `mode1_standard.cpp` 同步。
- 若后续继续补表情或数位板模式，也继续从同一上游资源链补充。
