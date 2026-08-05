#!/bin/bash
# macOS 构建后补丁：向 Info.plist 注入权限描述文案
# 在 tauri build 完成后自动执行
# 这些描述会显示在 macOS 系统授权弹窗中

APP_BUNDLE=$(find target -name "Work Review.app" -type d 2>/dev/null | head -1)

if [ -z "$APP_BUNDLE" ]; then
    echo "[patch-info-plist] 未找到 .app 包，跳过"
    exit 0
fi

PLIST="$APP_BUNDLE/Contents/Info.plist"

if [ ! -f "$PLIST" ]; then
    echo "[patch-info-plist] Info.plist 不存在: $PLIST"
    exit 0
fi

echo "[patch-info-plist] 正在注入权限描述到: $PLIST"

# 屏幕录制权限描述
/usr/libexec/PlistBuddy -c "Add :NSScreenCaptureUsageDescription string 'Work Review 需要屏幕录制权限来定时截取屏幕截图，用于记录您的工作活动。所有数据仅保存在本地。'" "$PLIST" 2>/dev/null \
  || /usr/libexec/PlistBuddy -c "Set :NSScreenCaptureUsageDescription 'Work Review 需要屏幕录制权限来定时截取屏幕截图，用于记录您的工作活动。所有数据仅保存在本地。'" "$PLIST"

# AppleScript 自动化权限描述
/usr/libexec/PlistBuddy -c "Add :NSAppleEventsUsageDescription string 'Work Review 需要自动化权限来获取当前活动窗口信息和浏览器 URL，用于生成工作日报。'" "$PLIST" 2>/dev/null \
  || /usr/libexec/PlistBuddy -c "Set :NSAppleEventsUsageDescription 'Work Review 需要自动化权限来获取当前活动窗口信息和浏览器 URL，用于生成工作日报。'" "$PLIST"

echo "[patch-info-plist] 注入完成"
