# Release Notes（多语言）

每个版本的**多语言 release notes** 存放在此目录，采用「主体 + 独立语言文件 + 链接切换」模式（参考 [cc-switch](https://github.com/farion1231/cc-switch)）。

## 机制

- **中文主体**：由 `.github/workflows/release.yml` 的 `Extract changelog` 步骤从 `CHANGELOG.md` 对应版本段落自动提取，作为 GitHub Release 的 body。
- **英文 / 繁体版本**：作为独立文件放在本目录；发布时 CI 自动在 Release body 顶部生成指向它们的切换链接。

Release body 形如：

```markdown
**[English →](./v1.0.51-en.md) | [繁體中文 →](./v1.0.51-tw.md)**

---

（以下为中文主体，从 CHANGELOG 自动提取）

### 修复
- ...

### macOS 安装说明
...
```

## 命名规范

针对版本 `1.0.51`（tag `v1.0.51`）：

| 文件 | 语言 | 必需性 |
|---|---|---|
| `v1.0.51-en.md` | English | 可选（存在则自动加链接） |
| `v1.0.51-tw.md` | 繁體中文 | 可选（存在则自动加链接） |

中文不需要独立文件——主体已在 Release body 与 `CHANGELOG.md`。

## 为新版本添加多语言 notes

1. 在 `CHANGELOG.md` 写好该版本中文变更（`## [1.0.51] - YYYY-MM-DD`）—— 这是主体，CI 自动提取。
2. 在本目录创建 `v1.0.51-en.md` / `v1.0.51-tw.md`，翻译该版本内容。
3. **先提交这些文件，再打 tag** `v1.0.51` 并推送——CI 在 tag 对应的 commit 里查找语言文件。
4. CI 自动在 Release body 顶部加入 `English / 繁體中文` 切换链接。

某版本若没有 en/tw 文件，Release body 就只有中文主体（向后兼容，不会报错）。
