# Work Review 安装脚本

两套安装方案，按你的需要挑一套。**强烈建议用 `deb/`**（见下方对比）。

## 目录结构

```
scripts/
├── deb/                  ← 推荐
│   ├── reinstall.sh        安装 / 升级 / 重装（幂等；自动清理任何旧方案残留）
│   └── uninstall.sh        卸载
└── appimage/
    ├── install.sh          AppImage 方案安装
    └── uninstall.sh        AppImage 方案卸载
```

## 两套方案对比

| | `deb/`（推荐） | `appimage/` |
|---|---|---|
| 依赖怎么来 | apt 自动拉（tesseract、gnome-screenshot、webkit、...） | 脚本 apt 自动装 + 再装 5 个 shim 绕过 `LD_LIBRARY_PATH` 污染 |
| 产出文件 | `/usr/bin/Work_Review` + `/usr/share/applications/` + `~/.local/bin/gnome-screenshot` (no-flash shim) + `~/.local/share/sounds/{Yaru,freedesktop}/stereo/screen-capture.oga` (静音覆盖) | `~/Applications/*.AppImage` + `~/bin/` + `~/.local/share/applications/` |
| GNOME 活动搜索 | 自带 desktop 入口 | 脚本生成 desktop 入口 |
| 卸载复杂度 | `sudo apt remove work-review` + 删 shim/静音文件/扩展 | 逐个删 shim/launcher/desktop/AppImage |
| 系统截图是否闪屏 / 咔擦响 | 不闪、不响（shim + 静音 oga 覆盖；详见 work-review-debugging.md "10 秒闪屏 + 咔擦"小节） | 不闪、不响（同 deb 方案的方法） |

## 典型场景

### (推荐) 安装 / 升级 / 重装，deb 方案
```bash
bash scripts/deb/reinstall.sh
```
脚本会自动：
1. 检测系统现状（有没有旧 AppImage、旧 deb、shim、扩展、运行进程）
2. 清理旧痕迹（停进程、`apt remove`、删 `~/Applications/` 下的 AppImage、删旧 shim），**保留 `~/.local/share/work-review/` 下的历史数据**
3. 下载 deb → `apt install`（自动拉所有依赖）
4. **GNOME 46 截屏修复**：装 no-flash `gnome-screenshot` shim（`~/.local/bin/`）+ 静音 `screen-capture.oga` 覆盖（`~/.local/share/sounds/{Yaru,freedesktop}/stereo/`）。两者一起解决 Ubuntu 24.04 自带 gnome-screenshot 41 在 GNOME 46 上"每次截屏闪一下 + 响一声"的问题
5. 装 GNOME 扩展 `focused-window-dbus`
6. 末尾提示是否需要注销重登（首次装扩展必须）

> 这个脚本从"干净系统"、"已装 AppImage"、"已装 deb"任意状态运行都能得到一致结果。

### 卸载 deb 方案
```bash
bash scripts/deb/uninstall.sh              # 保留历史数据
bash scripts/deb/uninstall.sh --purge      # 连数据也删
bash scripts/deb/uninstall.sh --dry-run    # 预演一遍不动真格
```

### AppImage 方案（如果你非要用）
```bash
# 需要先把 Work_Review_*.AppImage 放到 ~/Applications/ 或项目根目录
bash scripts/appimage/install.sh            # 装（自动扫 AppImage，apt 装依赖，部署 shim + launcher + desktop）
bash scripts/appimage/uninstall.sh          # 卸（保留数据）
bash scripts/appimage/uninstall.sh --purge  # 卸（连数据也删）
```

### 通用参数
所有脚本都支持 `--dry-run`，显示将做哪些动作，不真执行。  
卸载脚本用 `--purge` 连用户数据也删；`deb/reinstall.sh` 对应的是 `--purge-data`。

## 可用环境变量

| 变量 | 作用 | 默认 |
|---|---|---|
| `WR_DEB` | 本地 .deb 文件路径（避免下载） | 自动扫**当前工作目录**的 `Work_Review_*.deb`（任意版本） |
| `WR_DEB_URL` | .deb 下载 URL | 通过 GitHub API 解析 `wm94i/Work-Review` 的最新 release |
| `WR_APPIMAGE` | (appimage 方案) 本地 AppImage 路径 | 自动扫 `~/Applications/` 和项目根目录 |

## 为啥要装 GNOME 扩展 focused-window-dbus？

Wayland 没有"拿当前聚焦窗口"的公开 API（隐私设计），所以 Work Review 通过一个 GNOME Shell 扩展把这个信息通过 DBus 暴露出来。扩展用的是 commit `0368030`（支持 GNOME 45/46/47，master 只支持 49+）。

Wayland 下新装的扩展必须注销重登 Shell 才会被加载——这是 GNOME 本身的限制，脚本会提示。
