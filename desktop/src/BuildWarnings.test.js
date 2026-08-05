import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';

function runBuild() {
  return new Promise((resolve) => {
    const child = spawn('npm', ['run', 'build'], {
      cwd: new URL('..', import.meta.url),
      env: { ...process.env, FORCE_COLOR: '0' },
      stdio: ['ignore', 'pipe', 'pipe'],
      shell: true,
    });

    let output = '';
    child.stdout.on('data', (chunk) => {
      output += chunk.toString();
    });
    child.stderr.on('data', (chunk) => {
      output += chunk.toString();
    });
    child.on('close', (code) => {
      resolve({ code, output });
    });
  });
}

test('生产构建不应出现已知质量警告', async () => {
  const { code, output } = await runBuild();

  assert.equal(code, 0, output);
  assert.doesNotMatch(output, /\[vite-plugin-svelte\].*A11y:/);
  assert.doesNotMatch(output, /Some chunks are larger than 500 kB after minification/);
  assert.doesNotMatch(output, /Browserslist: browsers data .* is .* old/);
  assert.doesNotMatch(output, /warnings when minifying css/);
  assert.doesNotMatch(output, /css-syntax-error/);
  assert.doesNotMatch(output, /Expected identifier but found/);
});
