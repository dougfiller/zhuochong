import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';

async function readCommandsSource() {
  // commands.rs 已按领域拆分为 commands/*.rs，这里拼接所有子模块以保持断言语义不变。
  const dir = new URL('./src/commands/', import.meta.url);
  const files = (await readdir(dir)).filter((f) => f.endsWith('.rs'));
  const parts = await Promise.all(files.map((f) => readFile(new URL(f, dir), 'utf8')));
  return parts.join('\n');
}

test('Linux 后端应识别 X11 与 Wayland 会话类型', async () => {
  const source = await readFile(new URL('./src/linux_session.rs', import.meta.url), 'utf8');

  assert.match(source, /pub enum LinuxDesktopSession/);
  assert.match(source, /pub enum LinuxDesktopEnvironment/);
  assert.match(source, /Wayland/);
  assert.match(source, /X11/);
  assert.match(source, /Gnome/);
  assert.match(source, /XDG_SESSION_TYPE/);
  assert.match(source, /WAYLAND_DISPLAY/);
  assert.match(source, /DISPLAY/);
  assert.match(source, /XDG_CURRENT_DESKTOP/);
  assert.match(source, /DESKTOP_SESSION/);
  assert.match(source, /current_linux_desktop_environment/);
});

test('Linux 截图应按会话类型分流，Wayland 优先尝试原生工具', async () => {
  const source = await readFile(new URL('./src/screenshot.rs', import.meta.url), 'utf8');

  assert.match(source, /LinuxDesktopSession::Wayland/);
  assert.match(source, /capture_linux_wayland/);
  assert.match(source, /grim/);
  assert.match(source, /gnome-screenshot/);
  assert.match(source, /spectacle/);
});

test('Linux 活动窗口检测应为 GNOME Wayland 单独提供 provider 分流', async () => {
  const source = await readFile(new URL('./src/monitor.rs', import.meta.url), 'utf8');

  assert.match(source, /current_linux_active_window_provider/);
  assert.match(source, /get_active_window_linux_x11/);
  assert.match(source, /get_active_window_linux_wayland_gnome/);
  assert.match(source, /get_active_window_linux_wayland_kde/);
  assert.match(source, /get_active_window_linux_wayland_sway/);
  assert.match(source, /get_active_window_linux_wayland_hyprland/);
  assert.match(source, /LinuxDesktopEnvironment::Gnome/);
  assert.match(source, /org\.gnome\.shell\.extensions\.FocusedWindow\.Get/);
  assert.match(source, /gdbus/);
  assert.match(source, /kdotool/);
  assert.match(source, /swaymsg/);
  assert.match(source, /hyprctl/);
  assert.match(source, /WindowBounds/);
  assert.match(source, /parse_xdotool_geometry_shell_output/);
  assert.match(source, /parse_window_bounds_from_json/);
  assert.match(source, /resolve_browser_url_for_window_linux/);
  assert.match(source, /firefox_family_session_store_url/);
});

test('GNOME Wayland 桌宠联动应内置专用扩展并通过 D-Bus 暴露指针信息', async () => {
  const inputSource = await readFile(new URL('./src/avatar_input.rs', import.meta.url), 'utf8');
  const commandSource = await readCommandsSource();
  const metadata = await readFile(
    new URL('../scripts/gnome-shell/work-review-avatar-input@workreview.app/metadata.json', import.meta.url),
    'utf8'
  );
  const extensionSource = await readFile(
    new URL('../scripts/gnome-shell/work-review-avatar-input@workreview.app/extension.js', import.meta.url),
    'utf8'
  );

  assert.match(inputSource, /WorkReviewAvatarInput\.GetInput/);
  assert.match(inputSource, /gnome-shell-dbus/);
  assert.match(commandSource, /install_gnome_avatar_extension/);
  assert.match(commandSource, /requires_relogin/);
  assert.match(commandSource, /gnome_avatar_extension_needs_relogin/);
  assert.match(commandSource, /gnome-extensions/);
  assert.match(commandSource, /work-review-avatar-input@workreview\.app/);
  assert.match(metadata, /work-review-avatar-input@workreview\.app/);
  assert.match(extensionSource, /global\.get_pointer\(\)/);
  assert.match(extensionSource, /captured-event/);
  assert.match(extensionSource, /KEY_PRESS/);
  assert.match(extensionSource, /get_key_symbol/);
  assert.match(extensionSource, /org\.gnome\.shell\.extensions\.WorkReviewAvatarInput/);
  assert.match(extensionSource, /keyboardTimestampMs/);
  assert.match(extensionSource, /keyval/);
});
