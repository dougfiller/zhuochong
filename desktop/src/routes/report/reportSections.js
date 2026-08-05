const CJK_NUMBERS = ['一', '二', '三', '四', '五', '六', '七', '八', '九', '十'];
const BLOCK_START_RE = /^<!--\s*WR_BLOCK_START:(\w+)\s*-->/;
const NUMBERED_TITLE_RE = /^(?:[一二三四五六七八九十百千万]+、|\d+[.)]\s*)/;

function isSectionBoundary(line) {
  return line.startsWith('## ') || line.startsWith('<details>');
}

function isBlockStart(line) {
  return BLOCK_START_RE.test(line);
}

function buildSection(title, bodyLines, originalIndex) {
  return {
    title,
    body: bodyLines.join('\n'),
    originalIndex,
  };
}

export function parseReportSections(content) {
  if (!content) return [];

  const sections = [];
  const lines = content.split('\n');
  let currentTitle = '';
  let currentLines = [];
  let pendingPrefix = [];

  function pushCurrent() {
    if (!currentTitle && currentLines.length === 0) return;
    sections.push(buildSection(currentTitle, currentLines, sections.length));
    currentTitle = '';
    currentLines = [];
  }

  for (const line of lines) {
    if (isBlockStart(line)) {
      pushCurrent();
      pendingPrefix = [line];
      continue;
    }

    if (isSectionBoundary(line)) {
      pushCurrent();
      currentTitle = line.startsWith('## ') ? line : '';
      currentLines = line.startsWith('<details>')
        ? [...pendingPrefix, line]
        : [...pendingPrefix];
      pendingPrefix = [];
      continue;
    }

    if (pendingPrefix.length > 0 && !currentTitle && currentLines.length === 0) {
      currentLines.push(...pendingPrefix);
      pendingPrefix = [];
    }
    currentLines.push(line);
  }

  if (pendingPrefix.length > 0) {
    currentLines.push(...pendingPrefix);
  }
  pushCurrent();

  return sections;
}

export function extractReportBlockName(section) {
  const content = `${section?.body || ''}\n${section?.title || ''}`;
  const match = content.match(BLOCK_START_RE);
  return match ? match[1] : null;
}

export function getVisibleReportSections(sections, pinnedBlocks = [], hiddenBlocks = []) {
  const pinnedOrder = new Map(pinnedBlocks.map((blockName, index) => [blockName, index]));

  const sortedSections = sections
    .filter((section) => {
      const blockName = extractReportBlockName(section);
      return !blockName || !hiddenBlocks.includes(blockName);
    })
    .map((section, visibleOrder) => ({ ...section, visibleOrder }))
    .sort((left, right) => {
      const leftBlock = extractReportBlockName(left);
      const rightBlock = extractReportBlockName(right);
      const leftPinned = leftBlock ? pinnedOrder.get(leftBlock) : undefined;
      const rightPinned = rightBlock ? pinnedOrder.get(rightBlock) : undefined;

      if (leftPinned !== undefined && rightPinned !== undefined) {
        return leftPinned - rightPinned || left.visibleOrder - right.visibleOrder;
      }
      if (leftPinned !== undefined) return -1;
      if (rightPinned !== undefined) return 1;
      return left.visibleOrder - right.visibleOrder;
    });

  let displaySectionIndex = 0;
  return sortedSections.map((section) => {
    if (!section.title?.startsWith('## ')) {
      return { ...section, displaySectionIndex: null };
    }

    const indexedSection = { ...section, displaySectionIndex };
    displaySectionIndex += 1;
    return indexedSection;
  });
}

function sectionNumberPrefix(visibleIndex, localeCode) {
  if (localeCode === 'en') {
    return `${visibleIndex + 1}. `;
  }

  return `${CJK_NUMBERS[visibleIndex] || visibleIndex + 1}、`;
}

function renumberTitle(title, visibleIndex, localeCode) {
  if (!title?.startsWith('## ')) return title || '';

  const titleText = title.slice(3).replace(NUMBERED_TITLE_RE, '');
  return `## ${sectionNumberPrefix(visibleIndex, localeCode)}${titleText}`;
}

export function reportSectionMarkdownForDisplay(section, visibleIndex, localeCode) {
  const title = renumberTitle(section?.title || '', visibleIndex, localeCode);
  const body = section?.body || '';

  if (title && body) return `${title}\n${body}`;
  return title || body;
}

export function reportSectionMarkdownForStorage(section) {
  const title = section?.title || '';
  const body = section?.body || '';
  if (!body) return title;

  const lines = body.split('\n');
  const leadingMarkers = [];
  while (lines.length > 0 && isBlockStart(lines[0])) {
    leadingMarkers.push(lines.shift());
  }

  const parts = [...leadingMarkers];
  if (title) parts.push(title);
  if (lines.length > 0) parts.push(lines.join('\n'));
  return parts.join('\n');
}
