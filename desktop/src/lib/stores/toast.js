import { writable } from 'svelte/store';

const { subscribe, set, update } = writable(null);

let toastId = 0;
let hideTimer = null;

export const toast = {
  subscribe,
};

export function showToast(message, type = 'info', duration = 3000) {
  const normalizedMessage = typeof message === 'string' ? message.trim() : '';
  const normalizedLower = normalizedMessage.toLowerCase();

  if (
    !normalizedMessage ||
    normalizedLower === 'undefined' ||
    normalizedLower === 'null'
  ) {
    return;
  }

  toastId += 1;
  const currentId = toastId;

  set({
    id: currentId,
    message: normalizedMessage,
    type,
  });

  if (hideTimer) {
    clearTimeout(hideTimer);
  }

  hideTimer = setTimeout(() => {
    update((current) => (current?.id === currentId ? null : current));
    hideTimer = null;
  }, duration);
}

export function clearToast() {
  if (hideTimer) {
    clearTimeout(hideTimer);
    hideTimer = null;
  }

  set(null);
}
