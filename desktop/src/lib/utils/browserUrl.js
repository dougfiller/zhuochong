function isHexByte(value) {
  return /^[0-9a-fA-F]{2}$/.test(value);
}

function decodeNonAsciiPercentSequences(input) {
  let output = '';

  for (let index = 0; index < input.length; ) {
    if (input[index] !== '%') {
      output += input[index];
      index += 1;
      continue;
    }

    let cursor = index;
    const bytes = [];

    while (
      cursor + 2 < input.length &&
      input[cursor] === '%' &&
      isHexByte(input.slice(cursor + 1, cursor + 3))
    ) {
      bytes.push(Number.parseInt(input.slice(cursor + 1, cursor + 3), 16));
      cursor += 3;
    }

    if (bytes.length === 0) {
      output += input[index];
      index += 1;
      continue;
    }

    const rawSegment = input.slice(index, cursor);
    if (!bytes.some((value) => value >= 0x80)) {
      output += rawSegment;
      index = cursor;
      continue;
    }

    try {
      output += new TextDecoder('utf-8', { fatal: true }).decode(Uint8Array.from(bytes));
      index = cursor;
    } catch {
      output += rawSegment;
      index = cursor;
    }
  }

  return output;
}

export function formatBrowserUrlForDisplay(rawUrl) {
  if (typeof rawUrl !== 'string' || rawUrl.trim() === '') {
    return '';
  }

  return decodeNonAsciiPercentSequences(rawUrl.trim());
}
