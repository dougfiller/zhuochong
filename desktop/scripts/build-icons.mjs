import { mkdtemp, mkdir, rm, writeFile, copyFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import pngToIco from 'png-to-ico';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const projectRoot = path.resolve(__dirname, '..');

const masterIcon = path.join(projectRoot, 'src-tauri', 'icons', 'icon.png');
const windowsMasterIcon = path.join(
  projectRoot,
  'src-tauri',
  'icons',
  'windows-icon.png'
);
const tauriIconsDir = path.join(projectRoot, 'src-tauri', 'icons');
const publicIconsDir = path.join(projectRoot, 'public', 'icons');
const tauriCliPath = path.join(
  projectRoot,
  'node_modules',
  '.bin',
  process.platform === 'win32' ? 'tauri.cmd' : 'tauri'
);
const hasFfmpeg = commandExists('ffmpeg');
const hasSips = commandExists('sips');
const hasIconutil = commandExists('iconutil', ['-h']);
const hasTauriCli = existsSync(tauriCliPath);
const hasDedicatedWindowsIcon = existsSync(windowsMasterIcon);

const persistentTargets = [
  { size: 16, output: path.join(tauriIconsDir, '16x16.png') },
  { size: 32, output: path.join(tauriIconsDir, '32x32.png') },
  { size: 48, output: path.join(tauriIconsDir, '48x48.png') },
  { size: 64, output: path.join(tauriIconsDir, '64x64.png') },
  { size: 128, output: path.join(tauriIconsDir, '128x128.png') },
  { size: 256, output: path.join(tauriIconsDir, '128x128@2x.png') },
  { size: 256, output: path.join(tauriIconsDir, '256x256.png') },
  { size: 512, output: path.join(tauriIconsDir, '512x512.png') },
  { size: 64, output: path.join(tauriIconsDir, 'tray-icon.png') },
  { size: 128, output: path.join(publicIconsDir, '128x128.png') },
  { size: 256, output: path.join(publicIconsDir, '256x256.png') },
];

const windowsIcoSizes = [16, 20, 24, 32, 40, 48, 64, 128, 256];
const macosIconsetTargets = [
  { size: 16, name: 'icon_16x16.png' },
  { size: 32, name: 'icon_16x16@2x.png' },
  { size: 32, name: 'icon_32x32.png' },
  { size: 64, name: 'icon_32x32@2x.png' },
  { size: 128, name: 'icon_128x128.png' },
  { size: 256, name: 'icon_128x128@2x.png' },
  { size: 256, name: 'icon_256x256.png' },
  { size: 512, name: 'icon_256x256@2x.png' },
  { size: 512, name: 'icon_512x512.png' },
  { size: 1024, name: 'icon_512x512@2x.png' },
];

function commandExists(command, args = ['-version']) {
  const result = spawnSync(command, args, { stdio: 'ignore' });
  return !result.error;
}

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: projectRoot,
    encoding: 'utf8',
  });

  if (result.status !== 0) {
    const stderr = result.stderr?.trim();
    const stdout = result.stdout?.trim();
    throw new Error(
      [stderr, stdout].filter(Boolean).join('\n') || `${command} 执行失败`
    );
  }
}

function renderFilter(size) {
  if (size <= 24) {
    return `scale=${size}:${size}:flags=lanczos,unsharp=7:7:0.9:7:7:0.0`;
  }

  if (size <= 48) {
    return `scale=${size}:${size}:flags=lanczos,unsharp=5:5:0.75:5:5:0.0`;
  }

  if (size <= 64) {
    return `scale=${size}:${size}:flags=lanczos,unsharp=3:3:0.45:3:3:0.0`;
  }

  return `scale=${size}:${size}:flags=lanczos`;
}

async function buildPngWithFfmpeg(size, output, source) {
  run('ffmpeg', [
    '-y',
    '-i',
    source,
    '-vf',
    renderFilter(size),
    '-frames:v',
    '1',
    '-update',
    '1',
    '-pix_fmt',
    'rgba',
    output,
  ]);
}

async function buildPngWithSips(size, output, source) {
  run('sips', ['-z', String(size), String(size), source, '--out', output]);
}

async function buildPng(size, output, source = masterIcon) {
  if (hasFfmpeg) {
    await buildPngWithFfmpeg(size, output, source);
    return;
  }

  if (hasSips) {
    await buildPngWithSips(size, output, source);
    return;
  }

  throw new Error('缺少 ffmpeg 或 sips，无法生成图标资源');
}

async function buildIcns(tempDir) {
  if (!hasIconutil) {
    if (process.platform === 'darwin') {
      throw new Error('缺少 tauri CLI 或 iconutil，无法生成 macOS icns 图标');
    }

    return false;
  }

  const iconsetDir = path.join(tempDir, 'icon.iconset');
  await mkdir(iconsetDir, { recursive: true });

  for (const target of macosIconsetTargets) {
    await buildPng(target.size, path.join(iconsetDir, target.name));
  }

  run('iconutil', [
    '--convert',
    'icns',
    '--output',
    path.join(tauriIconsDir, 'icon.icns'),
    iconsetDir,
  ]);

  return true;
}

async function buildNativeIconsWithTauri(tempDir) {
  if (!hasTauriCli) {
    return false;
  }

  const tauriTempDir = path.join(tempDir, 'tauri-icons');
  await mkdir(tauriTempDir, { recursive: true });

  run(tauriCliPath, ['icon', masterIcon, '-o', tauriTempDir]);
  if (!hasDedicatedWindowsIcon) {
    await copyFile(
      path.join(tauriTempDir, 'icon.ico'),
      path.join(tauriIconsDir, 'icon.ico')
    );
  }
  await copyFile(
    path.join(tauriTempDir, 'icon.icns'),
    path.join(tauriIconsDir, 'icon.icns')
  );

  return true;
}

async function buildIco(tempDir) {
  const icoPngs = [];
  const source = hasDedicatedWindowsIcon ? windowsMasterIcon : masterIcon;

  for (const size of windowsIcoSizes) {
    const output = path.join(tempDir, `${size}.png`);
    await buildPng(size, output, source);
    icoPngs.push(output);
  }

  const ico = await pngToIco(icoPngs);
  await writeFile(path.join(tauriIconsDir, 'icon.ico'), ico);
}

async function main() {
  if (!existsSync(masterIcon)) {
    throw new Error(`未找到主图标：${masterIcon}`);
  }

  await mkdir(publicIconsDir, { recursive: true });

  const tempDir = await mkdtemp(path.join(tmpdir(), 'work-review-icons-'));

  try {
    for (const target of persistentTargets) {
      await buildPng(target.size, target.output);
    }

    const builtNativeIcons = await buildNativeIconsWithTauri(tempDir);
    await buildIco(tempDir);

    if (!builtNativeIcons) {
      await buildIcns(tempDir);
    }

    // 保持 public 下主图和 src-tauri 主图一致，避免应用内品牌图资源分叉
    await copyFile(masterIcon, path.join(projectRoot, 'public', 'icon.png'));

    console.log(
      builtNativeIcons
        ? '图标生成完成：原生图标已切换为 Tauri CLI 官方生成链路'
        : `图标生成完成：Windows ICO 尺寸 ${windowsIcoSizes.join(', ')}`
    );
  } finally {
    await rm(tempDir, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
