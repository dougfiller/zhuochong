const electronSvg = `data:image/svg+xml;utf8,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none">
    <rect x="6" y="6" width="36" height="36" rx="12" fill="#EEF4FF" />
    <path d="M16 24c0-6 3.6-10 8-10s8 4 8 10-3.6 10-8 10-8-4-8-10Z" stroke="#4F46E5" stroke-width="2.4" />
    <path d="M12 18c5.2 2.8 18.8 2.8 24 0" stroke="#4F46E5" stroke-width="2.4" stroke-linecap="round" />
    <path d="M12 30c5.2-2.8 18.8-2.8 24 0" stroke="#4F46E5" stroke-width="2.4" stroke-linecap="round" />
    <circle cx="24" cy="24" r="3.4" fill="#4F46E5" />
  </svg>`
)}`;

function encodeSvg(svg) {
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

const discoverSvg = encodeSvg(
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none">
    <rect x="4" y="4" width="40" height="40" rx="12" fill="#EEF6FF" />
    <circle cx="24" cy="24" r="9.5" stroke="#2563EB" stroke-width="2.5" />
    <path d="M24 16l3.4 6.8L34 26l-6.8 3.4L24 36l-3.4-6.6L14 26l6.6-3.2L24 16Z" fill="#60A5FA" />
  </svg>`
);

const mailSvg = encodeSvg(
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none">
    <rect x="4" y="4" width="40" height="40" rx="12" fill="#EFF6FF" />
    <rect x="10" y="13" width="28" height="22" rx="5" fill="#DBEAFE" stroke="#2563EB" stroke-width="2.2" />
    <path d="M12 16.5 24 25l12-8.5" stroke="#2563EB" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" />
  </svg>`
);

const authSvg = encodeSvg(
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none">
    <rect x="4" y="4" width="40" height="40" rx="12" fill="#F3F4F6" />
    <path d="M24 12c5.7 3.7 10 4.5 10 4.5v8.3c0 6.2-4.3 10.6-10 11.2-5.7-.6-10-5-10-11.2v-8.3S18.3 15.7 24 12Z" fill="#D1D5DB" stroke="#4B5563" stroke-width="2.1" stroke-linejoin="round" />
    <path d="M20.5 24.5v-2a3.5 3.5 0 1 1 7 0v2" stroke="#374151" stroke-width="2.1" stroke-linecap="round" />
    <rect x="18.5" y="24.5" width="11" height="8.5" rx="2.2" fill="#4B5563" />
  </svg>`
);

const fallbackIconMap = new Map([
  ['work review', '/icons/256x256.png'],
  ['work-review', '/icons/256x256.png'],
  ['work_review', '/icons/256x256.png'],
  ['electron', electronSvg],
  ['electron helper', electronSvg],
  ['discover', discoverSvg],
  ['mail', mailSvg],
  ['邮件', mailSvg],
  ['system authentication', authSvg],
  ['coreautha', authSvg],
]);

function normalizeName(appName) {
  return typeof appName === 'string' ? appName.trim().toLowerCase() : '';
}

function buildMonogramIcon(appName) {
  const normalized = normalizeName(appName);
  if (!normalized) return null;

  const words = normalized
    .replace(/[^a-z0-9\u4e00-\u9fa5]+/gi, ' ')
    .trim()
    .split(/\s+/)
    .filter(Boolean);

  let label = '';
  if (words.length >= 2) {
    label = `${words[0][0]}${words[1][0]}`;
  } else if (words.length === 1) {
    label = words[0].slice(0, words[0].match(/[\u4e00-\u9fa5]/) ? 1 : 2);
  }

  label = label.toUpperCase();
  if (!label) return null;

  let hash = 0;
  for (let i = 0; i < normalized.length; i += 1) {
    hash = normalized.charCodeAt(i) + ((hash << 5) - hash);
  }

  const hue = Math.abs(hash) % 360;
  const bg = `hsl(${hue} 70% 94%)`;
  const fg = `hsl(${hue} 55% 32%)`;
  const fontSize = label.length > 1 ? 19 : 22;

  return `data:image/svg+xml;utf8,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
      <rect x="4" y="4" width="40" height="40" rx="12" fill="${bg}" />
      <text x="24" y="29" text-anchor="middle" font-family="Segoe UI, Arial, sans-serif" font-size="${fontSize}" font-weight="700" fill="${fg}">${label}</text>
    </svg>`
  )}`;
}

export function getFallbackAppIcon(appName) {
  const normalized = normalizeName(appName);
  if (!normalized) return null;

  if (fallbackIconMap.has(normalized)) {
    return fallbackIconMap.get(normalized);
  }

  if (normalized.includes('work review') || normalized.includes('work-review') || normalized.includes('work_review')) {
    return '/icons/256x256.png';
  }

  if (normalized.includes('electron')) {
    return electronSvg;
  }

  return buildMonogramIcon(appName);
}

export function resolveAppIconSrc(appName, base64) {
  if (typeof base64 === 'string' && base64.length > 100) {
    return `data:image/png;base64,${base64}`;
  }

  return getFallbackAppIcon(appName);
}
