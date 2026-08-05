import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('分类 store 应对已知系统分类 key 使用当前语言翻译而不是后端中文名称', async () => {
  const source = await readFile(new URL('./categories.js', import.meta.url), 'utf8');

  assert.match(source, /translateCategoryLabel\(found\.key\)/);
  assert.match(source, /translatedCategoryName !== found\.key/);
  assert.match(source, /name: isKnownSystemCategory\s*\?\s*translatedCategoryName\s*:\s*\(found\.name \|\| translatedCategoryName\)/);
  assert.doesNotMatch(
    source,
    /name: found\.name \|\| translateCategoryLabel\(found\.key\)/,
    '不能直接优先使用后端返回的中文内置分类名，否则英文时间线 chip 会继续显示中文'
  );
});

test('语义分类 store 应对已知语义分类使用当前语言翻译而不是后端中文名称', async () => {
  const source = await readFile(new URL('./categories.js', import.meta.url), 'utf8');

  assert.match(source, /translatedSemanticCategoryName = translateSemanticCategoryLabel\(found\.key\)/);
  assert.match(source, /isKnownSemanticCategory = found\.is_system \|\| translatedSemanticCategoryName !== found\.key/);
  assert.match(
    source,
    /return isKnownSemanticCategory\s*\?\s*translatedSemanticCategoryName\s*:\s*\(found\.name \|\| translatedSemanticCategoryName\)/
  );
  assert.doesNotMatch(
    source,
    /return found\.name \|\| translateSemanticCategoryLabel\(found\.key\)/,
    '不能直接优先使用后端返回的中文语义分类名，否则英文网站分类会继续显示中文'
  );
});
